//! App-api service surface implementation.

use async_trait::async_trait;
use sdkwork_deploy_contract::{
    is_deploy_package_artifact_type, AppDatabaseMigrationPage, AppDatabaseMigrationResponse,
    AppDatabaseProfilePage, AppDatabaseProfileResponse, AppDeploymentPage, AppDeploymentResponse,
    AppDomainPage, AppEnvironmentPage, AppEnvironmentResponse, AppPage, AppReleasePage,
    AppReleaseResponse, AppRemixResponse, AppResponse, AppSourceResponse, AppSourceSpecPage,
    AppSourceSpecResponse, BindAppSourceSpecSourceRequest, BuildPage, BuildResponse,
    BuildTemplatePage, BuildTemplateResponse, ChannelPage, ChannelResponse, ChannelRolloutPage,
    ChannelRolloutResponse, CompleteDeployUploadSessionRequest, CreateAppDatabaseMigrationRequest,
    CreateAppDatabaseProfileRequest, CreateAppDeploymentRequest, CreateAppEnvironmentRequest,
    CreateAppReleaseRequest, CreateAppRequest, CreateAppSourceSpecRequest, CreateArtifactRequest,
    CreateBuildRequest, CreateBuildTemplateRequest, CreateCertificateRequest,
    CreateDeployUploadSessionRequest, CreateDomainHostnameRequest, CreateDomainZoneRequest,
    CreateEnvVariableRequest, CreateHealthCheckRequest, CreatePlatformTargetRequest,
    CreateSigningIdentityRequest, CreateSourceRepositoryRequest, DeployAppApi,
    DeployAppRequestContext, DeployServiceError, DeployServiceErrorKind, DeployServiceResult,
    DeployUploadSessionResponse, EnvironmentPromotionPage, EnvironmentPromotionResponse,
    ListAppsQuery, ListDomainZonesQuery, PackagePage, PackageResponse, PlatformTargetPage,
    PlatformTargetResponse, PromoteChannelRequest, PromoteEnvironmentRequest,
    RegisterPackageRequest, ReleaseStatus, RequestCertificateOrderRequest, SigningIdentityPage,
    SigningIdentityResponse, SourceRepositoryPage, SourceRepositoryResponse,
    UpdateAppDatabaseProfileRequest, UpdateAppEnvironmentRequest, UpdateAppRemixRequest,
    UpdateAppRequest, UpdateAppSourceRequest, UpdateAppSourceSpecRequest, UpdateBuildStateRequest,
    UpdateDomainHostnameRequest, UpdateDomainZoneRequest, UsageEventPage, UsageEventQuery,
    UPLOAD_SESSION_STATUS_CANCELLED, UPLOAD_SESSION_STATUS_COMPLETED,
};
use sdkwork_deploy_drive_port::{
    DeployDriveActor, DeployDriveScope, DriveRequestCredentials, PrepareDeployUploadCommand,
};

use crate::app_delivery::assert_app_reachable;
use crate::scaffold_market_access::{scaffold_read_access, ScaffoldReadAccess};
use crate::DeployService;

impl DeployService {
    pub(crate) fn require_tenant(context: &DeployAppRequestContext) -> DeployServiceResult<i64> {
        if context.tenant_id <= 0 {
            return Err(sdkwork_deploy_contract::DeployServiceError::forbidden(
                "app operations require tenant authorization",
            ));
        }
        Ok(context.tenant_id)
    }

    fn drive_credentials(context: &DeployAppRequestContext) -> DriveRequestCredentials {
        DriveRequestCredentials {
            auth_token: context.auth_token.clone(),
            access_token: context.access_token.clone(),
        }
    }

    /// Tenant and acting subject the Drive upload port must scope to.
    ///
    /// The in-process port hands the same identity to Drive's own ACL that the
    /// removed Drive App API call derived from the authenticated request.
    fn drive_scope(context: &DeployAppRequestContext) -> DeployDriveScope {
        DeployDriveScope {
            tenant_id: context.tenant_id,
            operator: context
                .actor_id
                .filter(|actor_id| *actor_id > 0)
                .map(|actor_id| DeployDriveActor::user(actor_id.to_string())),
        }
    }

    /// Requests the order that turns a freshly created certificate into a version.
    ///
    /// Nothing else did. The renewal sweep only looks at certificates that already carry
    /// a validity window, and the renew endpoint refused a certificate that was not
    /// `ACTIVE` or `FAILED`, so a row written `PENDING` by creation was declined by every
    /// candidate path and never issued.
    ///
    /// The order is opened through `open_certificate_order`, so a console request
    /// inherits the CAA pre-flight, the ACME account resolution and the idempotency the
    /// operator endpoint and the renewal sweep already share. A second order-creation path
    /// would be a quiet way for this one to become the unregulated one.
    ///
    /// A refusal is logged and swallowed: the certificate is a legitimate accepted intent,
    /// a missing ACME account is a separate resource the operator can add, and `renew` can
    /// request the order again once it exists. Failing the request here would make
    /// certificate creation depend on unrelated setup.
    async fn request_first_certificate_order(
        &self,
        tenant_id: i64,
        certificate: &sdkwork_deploy_contract::CertificateResponse,
    ) {
        let request = RequestCertificateOrderRequest {
            certificate_id: certificate.id.clone(),
            // Anchored to the certificate, so replaying the creation replays the order
            // rather than opening a second one for one intent.
            idempotency_key: format!("initial:{}", certificate.id),
            challenge_type: None,
        };
        match self.open_certificate_order(tenant_id, &request).await {
            Ok(order) => tracing::info!(
                certificate = %certificate.id,
                order = %order.id,
                "certificate created and its first order requested"
            ),
            Err(error) => tracing::warn!(
                certificate = %certificate.id,
                error = %error,
                "certificate created but its first order could not be requested; \
                 it stays PENDING and can be requested again by a renewal"
            ),
        }
    }

    fn validate_upload_request(
        request: &CreateDeployUploadSessionRequest,
    ) -> DeployServiceResult<()> {
        if request.file_name.trim().is_empty() {
            return Err(sdkwork_deploy_contract::DeployServiceError::validation(
                "fileName is required",
            ));
        }
        if request.idempotency_key.trim().is_empty() {
            return Err(sdkwork_deploy_contract::DeployServiceError::validation(
                "idempotencyKey is required",
            ));
        }
        if request.content_length <= 0 {
            return Err(sdkwork_deploy_contract::DeployServiceError::validation(
                "contentLength must be positive",
            ));
        }
        if request.content_type.trim().is_empty() {
            return Err(sdkwork_deploy_contract::DeployServiceError::validation(
                "contentType is required",
            ));
        }
        if !is_deploy_package_artifact_type(request.package_type) {
            return Err(sdkwork_deploy_contract::DeployServiceError::validation(
                "packageType must be a deployable artifact type between 1 and 5",
            ));
        }
        Ok(())
    }

    fn ensure_upload_session_mutable(
        stored: &DeployUploadSessionResponse,
    ) -> DeployServiceResult<()> {
        match stored.status {
            UPLOAD_SESSION_STATUS_COMPLETED => {
                Err(sdkwork_deploy_contract::DeployServiceError::conflict(
                    "upload session already completed",
                ))
            }
            UPLOAD_SESSION_STATUS_CANCELLED => {
                Err(sdkwork_deploy_contract::DeployServiceError::conflict(
                    "upload session already cancelled",
                ))
            }
            _ => Ok(()),
        }
    }

    fn upload_session_request_matches_stored(
        request: &CreateDeployUploadSessionRequest,
        stored: &DeployUploadSessionResponse,
    ) -> bool {
        stored.app_id == request.app_id
            && stored.package_type == request.package_type
            && stored.file_name == request.file_name
            && stored.content_type == request.content_type
            && stored.content_length == request.content_length
            && stored.checksum == request.checksum
    }

    /// The deploy application this caller's credential proves, if any, **as a
    /// data-access binding**.
    ///
    /// `APPLICATION_IDENTITY_SPEC.md` §4.2 makes a machine credential's `app_id`
    /// the deploy application identity, and §4.3 requires its data-plane queries
    /// to filter by `(tenant_id, app_id)`. An interactive user session is not
    /// that credential: its `app_id` is the application the user signed in
    /// through (the tenant console is itself a deploy application), so binding
    /// data access to it rejected every cross-application console read with
    /// `40301`. [`DeployAppRequestContext::enforced_app_scope`] draws the line;
    /// the tenant/owner gate keeps deciding a user session's reach.
    fn caller_app_scope(context: &DeployAppRequestContext) -> Option<&str> {
        context.enforced_app_scope()
    }

    /// Proves that a write may only name the application the credential proves,
    /// and answers the `app_id` the row must be attributed to.
    ///
    /// Cases, and none of them widens an existing boundary:
    ///
    /// * no application binding in the credential — the body's `appId` stands,
    ///   which is the pre-existing tenant/owner-scoped behaviour (and the
    ///   console's, whose session is not an application binding);
    /// * application binding present, body silent — the row is attributed to the
    ///   caller's own application, so it stays readable by that caller instead of
    ///   becoming an un-attributed row;
    /// * application binding present, body names the same application — accepted;
    /// * application binding present, body names a different application in the
    ///   **same identity form** — refused. Silently rewriting the field would
    ///   answer with a row the caller did not ask for, which is worse than a 403;
    /// * the two values are different identity forms of the same identity — the
    ///   credential claim is the deploy application **anchor** (`deploy_app.id`,
    ///   decimal) while the body/path address is `deploy_app.uuid`. They cannot
    ///   be compared without the row, so the value is carried through and the
    ///   row-level gate (`assert_app_reachable`, which sees `deploy_application_id`
    ///   and `id`) makes the authoritative decision. Rejecting here would refuse
    ///   an application its own data.
    fn bind_caller_app_scope(
        context: &DeployAppRequestContext,
        requested_app_id: Option<&str>,
    ) -> DeployServiceResult<Option<String>> {
        let requested = requested_app_id
            .map(str::trim)
            .filter(|value| !value.is_empty());
        match (Self::caller_app_scope(context), requested) {
            (None, requested) => Ok(requested.map(str::to_owned)),
            (Some(scope), None) => Ok(Some(scope.to_owned())),
            (Some(scope), Some(requested)) if requested == scope => Ok(Some(scope.to_owned())),
            (Some(scope), Some(requested)) if app_identity_forms_differ(scope, requested) => {
                Ok(Some(requested.to_owned()))
            }
            (Some(_), Some(_)) => Err(sdkwork_deploy_contract::DeployServiceError::forbidden(
                "a credential bound to one application cannot act on another application",
            )),
        }
    }

    /// Requires that a **path-addressed** application is the one the machine
    /// credential proves (`APPLICATION_IDENTITY_SPEC.md` §4.1: a token must not
    /// be accepted by another application).
    ///
    /// The path segment is the same untrusted input a body field is, so it is
    /// checked against the verified principal rather than trusted: a credential
    /// bound to application A may address A and nothing else, for reads exactly
    /// as for writes. A credential that carries no application binding — a user
    /// session, or a token without an `app_id` claim — keeps the existing
    /// tenant/owner boundary, which the caller checks separately
    /// (`assert_app_access` / `assert_app_reachable`); this helper never replaces
    /// that gate, it runs in front of it.
    pub(crate) fn assert_caller_app_scope(
        context: &DeployAppRequestContext,
        addressed_app_id: &str,
    ) -> DeployServiceResult<()> {
        Self::bind_caller_app_scope(context, Some(addressed_app_id)).map(|_bound| ())
    }

    /// Runs one observation pass over a hostname's outstanding ownership proof.
    ///
    /// "Issue the challenge" and "check whether the operator published it" are
    /// the same operation seen from two sides: the first pass creates the
    /// attempt and hands back the record to publish, and every later pass
    /// re-reads DNS. That is why the batch claim endpoint reuses this verbatim —
    /// ADR-20260723 §3 forbids the caller from asserting success, so all this can
    /// ever do is report what the lookup saw.
    async fn advance_hostname_ownership(
        &self,
        tenant_id: i64,
        owner: sdkwork_deploy_contract::OwnershipReach,
        zone_id: &str,
        hostname_id: &str,
    ) -> DeployServiceResult<sdkwork_deploy_contract::DomainVerifyResponse> {
        let challenge = self
            .repository
            .domain_hostname_verification_challenge(tenant_id, owner, zone_id, hostname_id)
            .await?;
        // A challenge this call just opened cannot have a published record yet, so
        // there is nothing to look up: hand the operator the record to publish.
        // The test is `created`, not `token.is_some()`, because the token is now
        // re-derived on every reload and would otherwise make every reload skip
        // the lookup — which is the one thing that can move a name to VERIFIED.
        if challenge.verified || challenge.created {
            return Ok(challenge.response());
        }
        let verification_id = challenge.verification_id.as_deref().ok_or_else(|| {
            sdkwork_deploy_contract::DeployServiceError::Internal(
                "pending domain has no verification attempt".to_owned(),
            )
        })?;
        let proof_sha256 = challenge.proof_sha256.as_deref().ok_or_else(|| {
            sdkwork_deploy_contract::DeployServiceError::Internal(
                "pending domain verification has no proof digest".to_owned(),
            )
        })?;
        let observation = self
            .domain_ownership_verifier
            .verify_dns_txt(&challenge.hostname, proof_sha256)
            .await?;
        if !observation.matched {
            return Ok(challenge.response());
        }
        let observed_sha256 = observation.observed_sha256.as_deref().ok_or_else(|| {
            sdkwork_deploy_contract::DeployServiceError::Internal(
                "matched domain verification has no observed digest".to_owned(),
            )
        })?;
        if self
            .repository
            .confirm_domain_hostname_verification(
                tenant_id,
                owner,
                zone_id,
                hostname_id,
                verification_id,
                observed_sha256,
                &observation.verifier_identity,
            )
            .await?
        {
            return Ok(sdkwork_deploy_contract::DomainVerifyResponse {
                verified: true,
                method: crate::domain_verification::DOMAIN_VERIFICATION_METHOD_DNS_TXT.to_owned(),
                verification_id: Some(verification_id.to_owned()),
                record_name: challenge.record_name,
                record_relative_name: challenge.record_relative_name,
                token: None,
                expires_at: challenge.expires_at,
            });
        }
        let current = self
            .repository
            .domain_hostname_verification_challenge(tenant_id, owner, zone_id, hostname_id)
            .await?;
        if current.verified {
            Ok(current.response())
        } else {
            Err(sdkwork_deploy_contract::DeployServiceError::conflict(
                "domain verification challenge changed; retry with the current token",
            ))
        }
    }
}

/// Whether `value` is a `deploy_app.uuid` (canonical 8-4-4-4-12 hexadecimal).
fn is_deploy_app_uuid(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 36 {
        return false;
    }
    bytes.iter().enumerate().all(|(index, byte)| match index {
        8 | 13 | 18 | 23 => *byte == b'-',
        _ => byte.is_ascii_hexdigit(),
    })
}

/// Whether `value` is the decimal `deploy_app.id` anchor the IAM installation and
/// its tokens carry (`AppResponse.deployApplicationId`).
fn is_deploy_app_anchor(value: &str) -> bool {
    !value.is_empty() && value.len() <= 20 && value.bytes().all(|byte| byte.is_ascii_digit())
}

/// Whether two application identities are stated in the two forms that cannot be
/// compared without the row: the decimal anchor versus the uuid.
///
/// Two uuids, two anchors, or two arbitrary identifiers stay comparable, so a
/// same-form mismatch is still refused before any I/O.
fn app_identity_forms_differ(left: &str, right: &str) -> bool {
    (is_deploy_app_anchor(left) && is_deploy_app_uuid(right))
        || (is_deploy_app_uuid(left) && is_deploy_app_anchor(right))
}

fn normalize_optional_text(
    value: Option<String>,
    maximum_length: usize,
    field: &str,
) -> DeployServiceResult<Option<String>> {
    value
        .map(|value| {
            let value = value.trim().to_owned();
            if value.is_empty() || value.len() > maximum_length {
                return Err(sdkwork_deploy_contract::DeployServiceError::validation(
                    format!("{field} must contain 1 to {maximum_length} characters"),
                ));
            }
            Ok(value)
        })
        .transpose()
}

fn normalize_relative_hostname(
    relative_name: &str,
    apex_hostname: &str,
) -> DeployServiceResult<String> {
    let relative_name = relative_name.trim();
    if relative_name == "@" {
        return Ok("@".to_owned());
    }
    if relative_name.is_empty() || relative_name.ends_with('.') {
        return Err(sdkwork_deploy_contract::DeployServiceError::validation(
            "relativeName is invalid",
        ));
    }
    let hostname = crate::normalize_domain_hostname(&format!("{relative_name}.{apex_hostname}"))?;
    let suffix = format!(".{apex_hostname}");
    hostname
        .strip_suffix(&suffix)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| {
            sdkwork_deploy_contract::DeployServiceError::validation(
                "relativeName must remain inside the selected domain zone",
            )
        })
}

#[async_trait]
impl DeployAppApi for DeployService {
    async fn list_domain_zones(
        &self,
        context: &DeployAppRequestContext,
        query: &ListDomainZonesQuery,
    ) -> DeployServiceResult<sdkwork_deploy_contract::DomainZonePage> {
        let tenant_id = Self::require_tenant(context)?;
        if let Some(status) = query.status.as_deref() {
            if !matches!(status, "ACTIVE" | "PAUSED") {
                return Err(sdkwork_deploy_contract::DeployServiceError::validation(
                    "domain zone status is invalid",
                ));
            }
        }
        // The caller's own subject, not the tenant: the console lists the
        // domains this user maintains and nothing else.
        self.repository
            .list_domain_zones(tenant_id, context.ownership_reach(), query)
            .await
    }

    async fn create_domain_zone(
        &self,
        context: &DeployAppRequestContext,
        request: &CreateDomainZoneRequest,
    ) -> DeployServiceResult<sdkwork_deploy_contract::DomainZoneResponse> {
        let tenant_id = Self::require_tenant(context)?;
        let mut request = request.clone();
        request.apex_hostname = crate::normalize_zone_apex(&request.apex_hostname)?;
        request.display_name = normalize_optional_text(request.display_name, 200, "displayName")?;
        request.dns_provider = normalize_optional_text(request.dns_provider, 64, "dnsProvider")?;
        request.provider_zone_ref =
            normalize_optional_text(request.provider_zone_ref, 512, "providerZoneRef")?;
        // Proved before the row is written: a zone pinned to an account this caller
        // cannot see is a configuration error that would otherwise surface only when
        // the first wildcard certificate tried to present through it.
        request.provider_account_id = self
            .pin_provider_account(context, tenant_id, request.provider_account_id.as_deref())
            .await?;
        self.repository
            .create_domain_zone(
                tenant_id,
                context.organization_id,
                context.actor_id,
                &request,
            )
            .await
    }

    async fn retrieve_domain_zone(
        &self,
        context: &DeployAppRequestContext,
        zone_id: &str,
    ) -> DeployServiceResult<sdkwork_deploy_contract::DomainZoneResponse> {
        let tenant_id = Self::require_tenant(context)?;
        self.repository
            .retrieve_domain_zone(tenant_id, context.ownership_reach(), zone_id)
            .await
    }

    async fn update_domain_zone(
        &self,
        context: &DeployAppRequestContext,
        zone_id: &str,
        request: &UpdateDomainZoneRequest,
    ) -> DeployServiceResult<sdkwork_deploy_contract::DomainZoneResponse> {
        let tenant_id = Self::require_tenant(context)?;
        if let Some(status) = request.status.as_deref() {
            if !matches!(status, "ACTIVE" | "PAUSED") {
                return Err(sdkwork_deploy_contract::DeployServiceError::validation(
                    "domain zone status is invalid",
                ));
            }
        }
        let mut request = request.clone();
        request.display_name = normalize_optional_text(request.display_name, 200, "displayName")?;
        request.dns_provider = normalize_optional_text(request.dns_provider, 64, "dnsProvider")?;
        request.provider_zone_ref =
            normalize_optional_text(request.provider_zone_ref, 512, "providerZoneRef")?;
        // Tri-state: absent leaves the pin, empty clears it, anything else must name
        // an account this caller may bind. The clear marker stays an empty string
        // rather than becoming `None`, which the repository would read as "no change"
        // and silently ignore the operator's intent.
        match self
            .resolve_provider_account_update(
                context,
                tenant_id,
                request.provider_account_id.as_deref(),
            )
            .await?
        {
            None => {}
            Some(Some(account_id)) => request.provider_account_id = Some(account_id),
            Some(None) => request.provider_account_id = Some(String::new()),
        }
        self.repository
            .update_domain_zone(tenant_id, context.ownership_reach(), zone_id, &request)
            .await
    }

    async fn delete_domain_zone(
        &self,
        context: &DeployAppRequestContext,
        zone_id: &str,
    ) -> DeployServiceResult<()> {
        let tenant_id = Self::require_tenant(context)?;
        self.repository
            .delete_domain_zone(tenant_id, context.ownership_reach(), zone_id)
            .await
    }

    async fn list_domain_zone_verification_files(
        &self,
        context: &DeployAppRequestContext,
        zone_id: &str,
    ) -> DeployServiceResult<sdkwork_deploy_contract::DomainVerificationFilePage> {
        let tenant_id = Self::require_tenant(context)?;
        self.list_verification_files(tenant_id, context.ownership_reach(), zone_id)
            .await
    }

    async fn create_domain_zone_verification_file(
        &self,
        context: &DeployAppRequestContext,
        zone_id: &str,
        request: &sdkwork_deploy_contract::CreateDomainVerificationFileRequest,
    ) -> DeployServiceResult<sdkwork_deploy_contract::DomainVerificationFileResponse> {
        let tenant_id = Self::require_tenant(context)?;
        self.create_verification_file(tenant_id, context.ownership_reach(), zone_id, request)
            .await
    }

    async fn delete_domain_zone_verification_file(
        &self,
        context: &DeployAppRequestContext,
        zone_id: &str,
        file_id: &str,
    ) -> DeployServiceResult<()> {
        let tenant_id = Self::require_tenant(context)?;
        self.delete_verification_file(tenant_id, context.ownership_reach(), zone_id, file_id)
            .await
    }

    async fn check_domain_zone_verification_file(
        &self,
        context: &DeployAppRequestContext,
        zone_id: &str,
        file_id: &str,
    ) -> DeployServiceResult<sdkwork_deploy_contract::DomainVerificationFileCheckResponse> {
        let tenant_id = Self::require_tenant(context)?;
        self.check_verification_file(tenant_id, context.ownership_reach(), zone_id, file_id)
            .await
    }

    async fn list_domain_hostnames(
        &self,
        context: &DeployAppRequestContext,
        zone_id: &str,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<sdkwork_deploy_contract::DomainHostnamePage> {
        let tenant_id = Self::require_tenant(context)?;
        self.repository
            .list_domain_hostnames(
                tenant_id,
                context.ownership_reach(),
                zone_id,
                page,
                page_size,
            )
            .await
    }

    async fn create_domain_hostname(
        &self,
        context: &DeployAppRequestContext,
        zone_id: &str,
        request: &CreateDomainHostnameRequest,
    ) -> DeployServiceResult<sdkwork_deploy_contract::DomainHostnameResponse> {
        let tenant_id = Self::require_tenant(context)?;
        // Resolved as this caller's own zone: a zone they do not own is
        // reported as missing rather than lending its apex to the request.
        let zone = self
            .repository
            .retrieve_domain_zone(tenant_id, context.ownership_reach(), zone_id)
            .await?;
        let relative_name =
            normalize_relative_hostname(&request.relative_name, &zone.apex_hostname)?;
        self.repository
            .create_domain_hostname(
                tenant_id,
                context.ownership_reach(),
                zone_id,
                &CreateDomainHostnameRequest { relative_name },
            )
            .await
    }

    async fn retrieve_domain_hostname(
        &self,
        context: &DeployAppRequestContext,
        zone_id: &str,
        hostname_id: &str,
    ) -> DeployServiceResult<sdkwork_deploy_contract::DomainHostnameResponse> {
        let tenant_id = Self::require_tenant(context)?;
        self.repository
            .retrieve_domain_hostname(tenant_id, context.ownership_reach(), zone_id, hostname_id)
            .await
    }

    async fn delete_domain_hostname(
        &self,
        context: &DeployAppRequestContext,
        zone_id: &str,
        hostname_id: &str,
    ) -> DeployServiceResult<()> {
        let tenant_id = Self::require_tenant(context)?;
        self.repository
            .delete_domain_hostname(tenant_id, context.ownership_reach(), zone_id, hostname_id)
            .await
    }

    async fn update_domain_hostname(
        &self,
        context: &DeployAppRequestContext,
        zone_id: &str,
        hostname_id: &str,
        request: &UpdateDomainHostnameRequest,
    ) -> DeployServiceResult<sdkwork_deploy_contract::DomainHostnameResponse> {
        let tenant_id = Self::require_tenant(context)?;
        let zone = self
            .repository
            .retrieve_domain_zone(tenant_id, context.ownership_reach(), zone_id)
            .await?;
        let relative_name =
            normalize_relative_hostname(&request.relative_name, &zone.apex_hostname)?;
        self.repository
            .update_domain_hostname(
                tenant_id,
                context.ownership_reach(),
                zone_id,
                hostname_id,
                &UpdateDomainHostnameRequest { relative_name },
            )
            .await
    }

    async fn verify_domain_hostname(
        &self,
        context: &DeployAppRequestContext,
        zone_id: &str,
        hostname_id: &str,
    ) -> DeployServiceResult<sdkwork_deploy_contract::DomainVerifyResponse> {
        let tenant_id = Self::require_tenant(context)?;
        self.advance_hostname_ownership(tenant_id, context.ownership_reach(), zone_id, hostname_id)
            .await
    }

    async fn list_domain_zone_dns_records(
        &self,
        context: &DeployAppRequestContext,
        zone_id: &str,
        query: &sdkwork_deploy_contract::ListDomainDnsRecordsQuery,
    ) -> DeployServiceResult<sdkwork_deploy_contract::DomainDnsRecordPage> {
        let tenant_id = Self::require_tenant(context)?;
        crate::domain_dns_records::list_zone_dns_records(
            &self.repository,
            tenant_id,
            context.ownership_reach(),
            zone_id,
            &crate::repository::DomainDnsRecordFilter {
                hostname_id: query.hostname_id.clone(),
                host: query.host.clone(),
                record_type: query.record_type.clone(),
            },
            query.page,
            query.page_size,
        )
        .await
    }

    async fn sync_domain_zone_dns_records(
        &self,
        context: &DeployAppRequestContext,
        zone_id: &str,
    ) -> DeployServiceResult<sdkwork_deploy_contract::DomainDnsSyncResponse> {
        let tenant_id = Self::require_tenant(context)?;
        crate::domain_dns_records::sync_zone_dns_records(
            &self.repository,
            &self.certificate_dns01,
            tenant_id,
            context.ownership_reach(),
            zone_id,
        )
        .await
    }

    /// Creates one record through the Zone's cloud account and joins it to the
    /// stored snapshot. Validated before the vendor is asked; an owner already
    /// carrying the zone suffix is refused where the apex is in hand.
    async fn create_domain_zone_dns_record(
        &self,
        context: &DeployAppRequestContext,
        zone_id: &str,
        request: &sdkwork_deploy_contract::CreateDomainDnsRecordRequest,
    ) -> DeployServiceResult<sdkwork_deploy_contract::DomainDnsRecordResponse> {
        let tenant_id = Self::require_tenant(context)?;
        let (resolved, write) = crate::domain_dns_records::resolve_zone_write_context(
            &self.repository,
            &self.certificate_dns01,
            tenant_id,
            context.ownership_reach(),
            zone_id,
        )
        .await?;
        let change =
            crate::domain_dns_records::normalize_dns_record_change(request, &write.zone_apex)?;
        let record = resolved
            .presenter
            .create_record(&resolved.zone_apex, &change)
            .await
            .map_err(crate::domain_dns_records::map_dns_sync_error)?;
        let hostname_assets = self
            .repository
            .list_domain_hostname_assets(tenant_id, context.ownership_reach(), zone_id)
            .await?;
        self.repository
            .insert_domain_zone_dns_record(
                tenant_id,
                context.ownership_reach(),
                zone_id,
                &crate::repository::DomainDnsRecordUpsert {
                    record_name: record.record_name.clone(),
                    record_type: record.record_type.clone(),
                    record_value: record.record_value.clone(),
                    ttl_seconds: record
                        .ttl_seconds
                        .and_then(|value| i32::try_from(value).ok()),
                    priority: record.priority.and_then(|value| i32::try_from(value).ok()),
                    record_line: record.record_line.clone(),
                    domain_id: crate::domain_dns_records::matched_domain(
                        &hostname_assets,
                        &record.record_name,
                    ),
                    dns_provider: write.provider,
                    provider_account_id: write.provider_account_id,
                    provider_record_ref: record.provider_record_ref.clone(),
                },
            )
            .await
    }

    /// Replaces one record on the provider, then in the stored row.
    async fn update_domain_zone_dns_record(
        &self,
        context: &DeployAppRequestContext,
        zone_id: &str,
        record_id: &str,
        request: &sdkwork_deploy_contract::UpdateDomainDnsRecordRequest,
    ) -> DeployServiceResult<sdkwork_deploy_contract::DomainDnsRecordResponse> {
        let tenant_id = Self::require_tenant(context)?;
        let (resolved, write) = crate::domain_dns_records::resolve_zone_write_context(
            &self.repository,
            &self.certificate_dns01,
            tenant_id,
            context.ownership_reach(),
            zone_id,
        )
        .await?;
        let record_ref = self
            .repository
            .domain_zone_dns_record_ref(tenant_id, context.ownership_reach(), zone_id, record_id)
            .await?
            .ok_or_else(|| {
                sdkwork_deploy_contract::DeployServiceError::NotFound(
                    "dns record not found".to_string(),
                )
            })?;
        let change =
            crate::domain_dns_records::normalize_dns_record_change(request, &write.zone_apex)?;
        let record = resolved
            .presenter
            .update_record(&resolved.zone_apex, &record_ref, &change)
            .await
            .map_err(crate::domain_dns_records::map_dns_sync_error)?;
        let hostname_assets = self
            .repository
            .list_domain_hostname_assets(tenant_id, context.ownership_reach(), zone_id)
            .await?;
        self.repository
            .update_domain_zone_dns_record(
                tenant_id,
                context.ownership_reach(),
                zone_id,
                record_id,
                &crate::repository::DomainDnsRecordUpsert {
                    record_name: record.record_name.clone(),
                    record_type: record.record_type.clone(),
                    record_value: record.record_value.clone(),
                    ttl_seconds: record
                        .ttl_seconds
                        .and_then(|value| i32::try_from(value).ok()),
                    priority: record.priority.and_then(|value| i32::try_from(value).ok()),
                    record_line: record.record_line.clone(),
                    domain_id: crate::domain_dns_records::matched_domain(
                        &hostname_assets,
                        &record.record_name,
                    ),
                    dns_provider: write.provider,
                    provider_account_id: write.provider_account_id,
                    provider_record_ref: record.provider_record_ref.clone(),
                },
            )
            .await
    }

    /// Deletes one record on the provider and removes the stored row.
    async fn delete_domain_zone_dns_record(
        &self,
        context: &DeployAppRequestContext,
        zone_id: &str,
        record_id: &str,
    ) -> DeployServiceResult<()> {
        let tenant_id = Self::require_tenant(context)?;
        let (resolved, _write) = crate::domain_dns_records::resolve_zone_write_context(
            &self.repository,
            &self.certificate_dns01,
            tenant_id,
            context.ownership_reach(),
            zone_id,
        )
        .await?;
        let record_ref = self
            .repository
            .domain_zone_dns_record_ref(tenant_id, context.ownership_reach(), zone_id, record_id)
            .await?
            .ok_or_else(|| {
                sdkwork_deploy_contract::DeployServiceError::NotFound(
                    "dns record not found".to_string(),
                )
            })?;
        resolved
            .presenter
            .delete_record(&resolved.zone_apex, &record_ref)
            .await
            .map_err(crate::domain_dns_records::map_dns_sync_error)?;
        self.repository
            .delete_domain_zone_dns_record(tenant_id, context.ownership_reach(), zone_id, record_id)
            .await
    }

    /// Pauses one record (暂停解析) or resumes it — the provider first, the
    /// stored row second, so a failed provider call never leaves the page
    /// claiming a state the vendor did not accept.
    async fn set_domain_zone_dns_record_status(
        &self,
        context: &DeployAppRequestContext,
        zone_id: &str,
        record_id: &str,
        request: &sdkwork_deploy_contract::DomainDnsRecordStatusRequest,
    ) -> DeployServiceResult<sdkwork_deploy_contract::DomainDnsRecordResponse> {
        let tenant_id = Self::require_tenant(context)?;
        let (resolved, _write) = crate::domain_dns_records::resolve_zone_write_context(
            &self.repository,
            &self.certificate_dns01,
            tenant_id,
            context.ownership_reach(),
            zone_id,
        )
        .await?;
        let record_ref = self
            .repository
            .domain_zone_dns_record_ref(tenant_id, context.ownership_reach(), zone_id, record_id)
            .await?
            .ok_or_else(|| {
                sdkwork_deploy_contract::DeployServiceError::NotFound(
                    "dns record not found".to_string(),
                )
            })?;
        resolved
            .presenter
            .set_record_status(&resolved.zone_apex, &record_ref, request.enabled)
            .await
            .map_err(crate::domain_dns_records::map_dns_sync_error)?;
        self.repository
            .set_domain_zone_dns_record_status(
                tenant_id,
                context.ownership_reach(),
                zone_id,
                record_id,
                request.enabled,
            )
            .await
    }

    /// Declares whatever is missing from the request and advances each hostname's
    /// proof once, so an order that covers `N` names costs one round trip instead
    /// of `2N`.
    ///
    /// Refolding the fully-qualified names into zone-relative ones happens here
    /// rather than in the caller: the wizard already holds a SAN list, and every
    /// consumer that re-derives the fold is a place for the `*.*.shop` double
    /// wildcard to come back. A name that does not fold into the selected zone
    /// fails the whole request **before** any row is written, because a partially
    /// declared set would leave the tenant owning hostnames nobody asked to keep.
    async fn ensure_domain_hostname_claims(
        &self,
        context: &DeployAppRequestContext,
        zone_id: &str,
        request: &sdkwork_deploy_contract::EnsureDomainHostnameClaimsRequest,
    ) -> DeployServiceResult<sdkwork_deploy_contract::DomainHostnameClaimBatchResponse> {
        let tenant_id = Self::require_tenant(context)?;
        if request.hostnames.is_empty() {
            return Err(sdkwork_deploy_contract::DeployServiceError::validation(
                "at least one hostname is required",
            ));
        }
        // The batch exists to prepare one certificate order, so it inherits that
        // order's identifier budget instead of inventing a second one.
        if request.hostnames.len() > sdkwork_deploy_contract::MAX_CERTIFICATE_IDENTIFIERS {
            return Err(sdkwork_deploy_contract::DeployServiceError::validation(
                "too many hostnames for one request",
            ));
        }
        let zone = self
            .repository
            .retrieve_domain_zone(tenant_id, context.ownership_reach(), zone_id)
            .await?;
        let mut relative_names = Vec::with_capacity(request.hostnames.len());
        for hostname in &request.hostnames {
            let relative_name = crate::relative_name_for_hostname(&zone.apex_hostname, hostname)?;
            if relative_names.contains(&relative_name) {
                // Two spellings of the same name would produce one row and two
                // identical entries in the caller's SAN plan; refusing is the
                // only way the response can stay an honest map of the request.
                return Err(sdkwork_deploy_contract::DeployServiceError::validation(
                    "hostnames must be unique",
                ));
            }
            relative_names.push(relative_name);
        }

        let mut items = Vec::with_capacity(relative_names.len());
        for relative_name in &relative_names {
            let hostname = self
                .repository
                .ensure_domain_hostname(
                    tenant_id,
                    context.ownership_reach(),
                    zone_id,
                    relative_name,
                )
                .await?;
            // A row that is already proven needs no lookup: DNS cannot un-verify
            // it, and re-asking would spend the attempt's budget on an answer
            // already known.
            let observation = if hostname.verification_status == "VERIFIED" {
                None
            } else {
                Some(
                    self.advance_hostname_ownership(
                        tenant_id,
                        context.ownership_reach(),
                        zone_id,
                        &hostname.id,
                    )
                    .await?,
                )
            };
            let (
                verified,
                dns_record_name,
                dns_record_relative_name,
                dns_record_type,
                dns_record_value,
                expires_at,
            ) = match observation {
                // Nothing is presented once a name is proven: there is no
                // record left to publish.
                None => (true, None, None, None, None, None),
                Some(response) => {
                    let record_name = response.record_name;
                    let presentable = record_name.is_some();
                    (
                        response.verified,
                        record_name,
                        // A record the zone does not own yields nothing to
                        // publish, so the relative name disappears with it.
                        response.record_relative_name,
                        presentable.then(|| "TXT".to_owned()),
                        response.token,
                        response.expires_at,
                    )
                }
            };
            items.push(sdkwork_deploy_contract::DomainHostnameClaimResponse {
                hostname,
                verified,
                dns_record_name,
                dns_record_relative_name,
                dns_record_type,
                dns_record_value,
                expires_at,
            });
        }
        Ok(sdkwork_deploy_contract::DomainHostnameClaimBatchResponse { items })
    }

    /// Accounts the caller may pick when configuring DNS automation.
    ///
    /// Reads the provider account center through the Deploy cloud account port, so
    /// the same account is usable from wherever it is convenient to manage it. This
    /// is also the "先判断是否已存在" pre-check: filtered by `dnsProvider` it answers
    /// whether the credential inputs can be skipped in favour of pinning an account
    /// that already exists.
    async fn list_cloud_accounts(
        &self,
        context: &DeployAppRequestContext,
        query: &sdkwork_deploy_contract::ListCloudAccountsQuery,
    ) -> DeployServiceResult<sdkwork_deploy_contract::CloudAccountPage> {
        use sdkwork_deploy_cloud_account_port::{
            dns_provider, ListCloudAccountsCommand, CAPABILITY_DNS, CLOUD_ACCOUNT_SCOPES,
        };

        let tenant_id = Self::require_tenant(context)?;
        // An unsupported family is refused rather than ignored: silently listing every
        // account would let the console offer, say, an object-storage account for a
        // form that can never present through it.
        let dns_provider = match query
            .dns_provider
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            Some(raw) => Some(dns_provider::normalize(raw).ok_or_else(|| {
                sdkwork_deploy_contract::DeployServiceError::validation(format!(
                    "dnsProvider `{raw}` is not one of {}",
                    dns_provider::ALL.join(", ")
                ))
            })?),
            None => None,
        };
        let scope_type = query
            .scope_type
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());
        if let Some(scope_type) = scope_type {
            if !CLOUD_ACCOUNT_SCOPES.contains(&scope_type) {
                return Err(sdkwork_deploy_contract::DeployServiceError::validation(
                    format!(
                        "scopeType must be one of {}",
                        CLOUD_ACCOUNT_SCOPES.join(", ")
                    ),
                ));
            }
        }
        let page = self
            .cloud_accounts
            .list_accounts(ListCloudAccountsCommand {
                tenant_id,
                user_id: context.actor_id,
                // Handed down as the *family*, never as the centre's `vendorCode`: one
                // family is written under several codes (`DNSPOD` under `dnspod` and
                // under the legacy `tencent` / `qcloud`), so narrowing by a single code
                // would hide an account the operator can still bind — and a hidden
                // account reads as "this provider has no account", which is the one
                // thing this list must not say. The port owns that vocabulary and
                // applies it before counting and paging, so `total` below is the
                // family's own count and page two continues where page one stopped.
                dns_family: dns_provider.map(str::to_owned),
                scope_type: scope_type.map(str::to_owned),
                mine: query.mine,
                include_platform: true,
                capability_code: Some(CAPABILITY_DNS.to_owned()),
                search: query
                    .keyword
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_owned),
                page: query.page,
                page_size: query.page_size,
            })
            .await?;
        Ok(sdkwork_deploy_contract::CloudAccountPage {
            items: page
                .items
                .iter()
                // Already narrowed to the family by the port, before the page was cut,
                // so there is nothing to filter here: filtering a page after it was
                // sliced would shorten it and shift every later page.
                .map(crate::cloud_accounts::cloud_account_response)
                .collect(),
            total: page.total,
            page: page.page,
            page_size: page.page_size,
        })
    }

    /// Registers an account from console input, reusing one that already matches.
    ///
    /// Reuse is decided by the account port, not here, because only the account
    /// center knows its own precedence. A reused account that already carries a
    /// credential keeps it: the console reports both facts and the operator can see
    /// that the secret they typed was not applied.
    async fn create_cloud_account(
        &self,
        context: &DeployAppRequestContext,
        request: &sdkwork_deploy_contract::CreateCloudAccountRequest,
    ) -> DeployServiceResult<sdkwork_deploy_contract::CloudAccountRegistrationResponse> {
        use sdkwork_deploy_cloud_account_port::{dns_provider, RegisterCloudAccountCommand};

        let tenant_id = Self::require_tenant(context)?;
        let display_name = crate::cloud_accounts::required_text(
            &request.display_name,
            crate::cloud_accounts::MAXIMUM_DISPLAY_NAME_LENGTH,
            "displayName",
        )?;
        let family = dns_provider::normalize(&request.dns_provider).ok_or_else(|| {
            sdkwork_deploy_contract::DeployServiceError::validation(format!(
                "dnsProvider must be one of {}",
                dns_provider::ALL.join(", ")
            ))
        })?;
        let account_code = crate::cloud_accounts::account_code(&request.account_code, family)?;
        // The console cannot probe a credential — that would mean dispatching it
        // to the vendor from the read path — so it asks the operator to affirm the
        // credential is an active one for the declared vendor. Refused rather than
        // defaulted: an unattested credential that silently became usable is worse
        // than a refusal, because it looks configured and fails at the first order.
        if !request.confirms_credential {
            return Err(sdkwork_deploy_contract::DeployServiceError::validation(
                "confirmsCredential must be true: the operator has to affirm the credential is an active one for the declared provider",
            ));
        }
        let registration = self
            .cloud_accounts
            .register_account(RegisterCloudAccountCommand {
                tenant_id,
                organization_id: context.organization_id.unwrap_or_default(),
                actor_id: context.actor_id.unwrap_or_default(),
                user_id: context.actor_id,
                display_name,
                account_code,
                dns_provider: family.to_owned(),
                scope_type: request.scope_type.clone(),
                owner_user_id: None,
                environment: request.environment.clone(),
                is_default: request.is_default,
                access_key_id: request.access_key_id.clone().unwrap_or_default(),
                secret_access_key: request.secret_access_key.clone(),
                session_token: request.session_token.clone(),
            })
            .await?;
        Ok(sdkwork_deploy_contract::CloudAccountRegistrationResponse {
            account: crate::cloud_accounts::cloud_account_response(&registration.account),
            reused: registration.reused,
            credential_applied: registration.credential_applied,
        })
    }

    async fn update_app_composition(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        expected_app_version: i64,
        idempotency_key: &str,
        request: &sdkwork_deploy_contract::UpdateAppCompositionRequest,
    ) -> DeployServiceResult<sdkwork_deploy_contract::AppCompositionResponse> {
        self.update_composition(
            context,
            app_id,
            expected_app_version,
            idempotency_key,
            request,
        )
        .await
    }

    async fn activate_app(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
    ) -> DeployServiceResult<sdkwork_deploy_contract::AppResponse> {
        let tenant_id = Self::require_tenant(context)?;
        let current = self.repository.retrieve_app(tenant_id, app_id).await?;
        assert_app_reachable(context, &current)?;
        let app = self.repository.set_app_status(tenant_id, app_id, 1).await?;
        let _ = self.audit_app_action(context, "app.activate", app_id).await;
        Ok(app)
    }

    async fn pause_app(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
    ) -> DeployServiceResult<sdkwork_deploy_contract::AppResponse> {
        let tenant_id = Self::require_tenant(context)?;
        let current = self.repository.retrieve_app(tenant_id, app_id).await?;
        assert_app_reachable(context, &current)?;
        let app = self.repository.set_app_status(tenant_id, app_id, 2).await?;
        let _ = self.audit_app_action(context, "app.pause", app_id).await;
        Ok(app)
    }

    async fn app_factory_readiness(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
    ) -> DeployServiceResult<sdkwork_deploy_contract::AppFactoryReadiness> {
        self.factory_readiness(context, app_id).await
    }

    async fn uninstall_app(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        idempotency_key: &str,
    ) -> DeployServiceResult<sdkwork_deploy_contract::AppUninstallResponse> {
        self.uninstall_app(context, app_id, idempotency_key).await
    }

    async fn list_artifacts(
        &self,
        context: &DeployAppRequestContext,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<sdkwork_deploy_contract::ArtifactPage> {
        let tenant_id = Self::require_tenant(context)?;
        // The credential's application narrows the page to that application's
        // artifacts: without it the endpoint enumerates every artifact in the
        // tenant (file names, Drive nodes, checksums) to whichever application
        // asks first.
        self.repository
            .list_artifacts(tenant_id, Self::caller_app_scope(context), page, page_size)
            .await
    }

    async fn create_artifact(
        &self,
        context: &DeployAppRequestContext,
        request: &CreateArtifactRequest,
    ) -> DeployServiceResult<sdkwork_deploy_contract::ArtifactResponse> {
        let tenant_id = Self::require_tenant(context)?;
        // Refused before any row is written: a credential bound to application A
        // cannot register an artifact as application B's, which would otherwise
        // hand B an artifact it never published and A an artifact it can no
        // longer read.
        let app_scope = Self::bind_caller_app_scope(context, request.app_id.as_deref())?;
        if let Some(app_id) = app_scope.as_deref() {
            self.assert_app_access(context, tenant_id, app_id).await?;
        }
        if !is_deploy_package_artifact_type(request.package_type) {
            return Err(sdkwork_deploy_contract::DeployServiceError::validation(
                "packageType must identify a deployable package",
            ));
        }
        if request.file_name.trim().is_empty()
            || request.content_type.trim().is_empty()
            || request.content_length <= 0
            || request.drive_upload_session_id.trim().is_empty()
            || request.drive_space_id.trim().is_empty()
            || request.drive_node_id.trim().is_empty()
            || request.idempotency_key.trim().is_empty()
        {
            return Err(sdkwork_deploy_contract::DeployServiceError::validation(
                "file metadata, stable Drive references, and idempotencyKey are required",
            ));
        }
        self.repository
            .create_artifact_from_drive(tenant_id, app_scope.as_deref(), request)
            .await
    }

    async fn retrieve_artifact(
        &self,
        context: &DeployAppRequestContext,
        artifact_id: &str,
    ) -> DeployServiceResult<sdkwork_deploy_contract::ArtifactResponse> {
        let tenant_id = Self::require_tenant(context)?;
        let app_scope = Self::caller_app_scope(context);
        let artifact = self
            .repository
            .retrieve_artifact(tenant_id, app_scope, artifact_id)
            .await?;
        if let Some(app_id) = artifact.app_id.as_deref() {
            // An artifact attached to an app is readable through app scope. A
            // marketplace install is the other legitimate reader: the caller
            // does not own the author's app, but the marketplace sells the
            // published version that references this artifact.
            match self.assert_app_access(context, tenant_id, app_id).await {
                Ok(()) => {}
                Err(denied) if denied.kind() == DeployServiceErrorKind::NotFound => {
                    self.repository
                        .assert_marketplace_artifact_readable(tenant_id, artifact_id)
                        .await?;
                }
                Err(other) => return Err(other),
            }
        }
        Ok(artifact)
    }

    async fn retain_artifact(
        &self,
        context: &DeployAppRequestContext,
        artifact_id: &str,
    ) -> DeployServiceResult<()> {
        let tenant_id = Self::require_tenant(context)?;
        // Retiring is a write: the marketplace read exception that
        // `retrieve_artifact` grants does not apply, and the credential's
        // application narrows the row the UPDATE may touch.
        self.repository
            .retain_artifact(tenant_id, Self::caller_app_scope(context), artifact_id)
            .await
    }

    async fn list_env_variables(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        environment: Option<&str>,
    ) -> DeployServiceResult<sdkwork_deploy_contract::EnvVariablePage> {
        let tenant_id = Self::require_tenant(context)?;
        // The owner gate alone lets the same owner (and every tenant-level
        // caller) read another of their applications' variables; the credential's
        // own application narrows the addressable set first.
        Self::assert_caller_app_scope(context, app_id)?;
        self.assert_app_access(context, tenant_id, app_id).await?;
        self.repository
            .list_env_variables(tenant_id, app_id, environment)
            .await
    }

    async fn create_env_variable(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        request: &CreateEnvVariableRequest,
    ) -> DeployServiceResult<sdkwork_deploy_contract::EnvVariableResponse> {
        let tenant_id = Self::require_tenant(context)?;
        Self::assert_caller_app_scope(context, app_id)?;
        self.assert_app_access(context, tenant_id, app_id).await?;
        self.repository
            .create_env_variable(tenant_id, app_id, request)
            .await
    }

    async fn list_app_resource_links(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        query: &sdkwork_deploy_contract::ListAppResourceLinksQuery,
    ) -> DeployServiceResult<sdkwork_deploy_contract::AppResourceLinkPage> {
        // Forwarded to the inherent method so the scope gate, the owner gate and
        // the request normalization live in one place; a second implementation
        // here is how a route and a service drift apart.
        DeployService::list_app_resource_links(self, context, app_id, query).await
    }

    async fn upsert_app_resource_link(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        request: &sdkwork_deploy_contract::UpsertAppResourceLinkRequest,
    ) -> DeployServiceResult<sdkwork_deploy_contract::AppResourceLinkResponse> {
        DeployService::upsert_app_resource_link(self, context, app_id, request).await
    }

    async fn delete_app_resource_link(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        resource_link_id: &str,
    ) -> DeployServiceResult<()> {
        DeployService::delete_app_resource_link(self, context, app_id, resource_link_id).await
    }

    async fn list_certificates(
        &self,
        context: &DeployAppRequestContext,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<sdkwork_deploy_contract::CertificatePage> {
        let tenant_id = Self::require_tenant(context)?;
        self.repository
            .list_certificates(tenant_id, page, page_size)
            .await
    }

    async fn create_certificate(
        &self,
        context: &DeployAppRequestContext,
        idempotency_key: &str,
        request: &CreateCertificateRequest,
    ) -> DeployServiceResult<sdkwork_deploy_contract::CertificateResponse> {
        let tenant_id = Self::require_tenant(context)?;
        // Rejected here so the caller gets a 422 naming the field, instead of the
        // database's CHECK constraint surfacing as an opaque internal error.
        crate::certificate_renewal::validate_renew_before_days(request.renew_before_days)
            .map_err(sdkwork_deploy_contract::DeployServiceError::validation)?;
        let mut request = request.clone();
        // Proved before the row is written, so a pin this caller cannot use fails the
        // request the operator is looking at rather than the order that follows it.
        request.provider_account_id = self
            .pin_provider_account(context, tenant_id, request.provider_account_id.as_deref())
            .await?;
        let certificate = self
            .repository
            .create_certificate(
                tenant_id,
                context.organization_id,
                context.actor_id,
                idempotency_key,
                &request,
            )
            .await?;
        // Creating a certificate is an accepted intent; this is what turns the intent
        // into work. Without it the row is `PENDING` forever, because no other path
        // advances a certificate that has never been issued.
        self.request_first_certificate_order(tenant_id, &certificate)
            .await;
        Ok(certificate)
    }

    async fn list_certificate_renewals(
        &self,
        context: &DeployAppRequestContext,
        certificate_id: &str,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<sdkwork_deploy_contract::CertificateRenewalPage> {
        let tenant_id = Self::require_tenant(context)?;
        self.repository
            .list_certificate_renewals(tenant_id, certificate_id, page, page_size)
            .await
    }

    async fn retrieve_certificate(
        &self,
        context: &DeployAppRequestContext,
        certificate_id: &str,
    ) -> DeployServiceResult<sdkwork_deploy_contract::CertificateResponse> {
        let tenant_id = Self::require_tenant(context)?;
        self.repository
            .retrieve_certificate(tenant_id, certificate_id)
            .await
    }

    async fn delete_certificate(
        &self,
        context: &DeployAppRequestContext,
        certificate_id: &str,
    ) -> DeployServiceResult<()> {
        let tenant_id = Self::require_tenant(context)?;
        self.repository
            .delete_certificate(tenant_id, certificate_id)
            .await
    }

    async fn renew_certificate(
        &self,
        context: &DeployAppRequestContext,
        certificate_id: &str,
    ) -> DeployServiceResult<sdkwork_deploy_contract::CertificateResponse> {
        let tenant_id = Self::require_tenant(context)?;
        let certificate = self
            .repository
            .retrieve_certificate(tenant_id, certificate_id)
            .await?;
        // A certificate with no version has never been issued, so asking again means
        // "request it", not "replace it": there is no previous version to supersede and
        // no window to move, and recording a renewal would make the ledger describe a
        // handover that never happened. It is also the retry path for a first order that
        // a missing ACME account deferred, and asking twice replays the same order
        // rather than duplicating it.
        if certificate.current_version_id.is_none() {
            self.request_first_certificate_order(tenant_id, &certificate)
                .await;
            return Ok(certificate);
        }
        self.repository
            .renew_certificate(tenant_id, certificate_id)
            .await
    }

    async fn refresh_certificate(
        &self,
        context: &DeployAppRequestContext,
        certificate_id: &str,
    ) -> DeployServiceResult<sdkwork_deploy_contract::CertificateResponse> {
        let tenant_id = Self::require_tenant(context)?;
        // Read-only on purpose: "refresh" re-reads the aggregate (the read model
        // recomputes the validity phase, the renewal window and the running
        // attempt on every call) and must never be mistaken for a renewal
        // request, which is the separate `renew` command.
        self.repository
            .retrieve_certificate(tenant_id, certificate_id)
            .await
    }

    async fn list_certificate_challenges(
        &self,
        context: &DeployAppRequestContext,
        certificate_id: &str,
    ) -> DeployServiceResult<sdkwork_deploy_contract::CertificateChallengePresentationList> {
        let tenant_id = Self::require_tenant(context)?;
        self.repository
            .list_certificate_presentations(tenant_id, certificate_id)
            .await
    }

    async fn verify_certificate_challenge(
        &self,
        context: &DeployAppRequestContext,
        certificate_id: &str,
    ) -> DeployServiceResult<sdkwork_deploy_contract::CertificateChallengeReleaseResult> {
        let tenant_id = Self::require_tenant(context)?;
        let accepted = self
            .release_manual_presentation(tenant_id, certificate_id)
            .await?;
        Ok(sdkwork_deploy_contract::CertificateChallengeReleaseResult { accepted })
    }

    async fn list_health_checks(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
    ) -> DeployServiceResult<sdkwork_deploy_contract::HealthCheckPage> {
        let tenant_id = Self::require_tenant(context)?;
        // Two conjunctive gates, in cheapest-first order: the credential may only
        // address its own application, and the addressed application must be
        // reachable for this subject. Before this, the path `appId` was the only
        // input — any token in the tenant could read another application's
        // checks.
        Self::assert_caller_app_scope(context, app_id)?;
        self.assert_app_access(context, tenant_id, app_id).await?;
        self.repository.list_health_checks(tenant_id, app_id).await
    }

    async fn create_health_check(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        request: &CreateHealthCheckRequest,
    ) -> DeployServiceResult<sdkwork_deploy_contract::HealthCheckResponse> {
        let tenant_id = Self::require_tenant(context)?;
        Self::assert_caller_app_scope(context, app_id)?;
        self.assert_app_access(context, tenant_id, app_id).await?;
        self.repository
            .create_health_check(tenant_id, app_id, request)
            .await
    }

    async fn create_upload_session(
        &self,
        context: &DeployAppRequestContext,
        request: &CreateDeployUploadSessionRequest,
    ) -> DeployServiceResult<DeployUploadSessionResponse> {
        let tenant_id = Self::require_tenant(context)?;
        Self::validate_upload_request(request)?;
        // A credential bound to application A may only open an upload for A.
        // Registering one under B would attribute A's bytes (and the artifact
        // minted from them) to an application that never uploaded them.
        let app_scope = Self::bind_caller_app_scope(context, request.app_id.as_deref())?;
        if let Some(app_id) = app_scope.as_deref() {
            self.assert_app_access(context, tenant_id, app_id).await?;
        }
        if let Some(existing) = self
            .repository
            .find_upload_session_by_idempotency_key(
                tenant_id,
                app_scope.as_deref(),
                &request.idempotency_key,
            )
            .await?
        {
            if Self::upload_session_request_matches_stored(request, &existing) {
                return Ok(existing);
            }
            return Err(sdkwork_deploy_contract::DeployServiceError::conflict(
                "idempotencyKey already used with a different upload payload",
            ));
        }
        let drive_response = self
            .drive
            .prepare_package_upload(
                &Self::drive_credentials(context),
                PrepareDeployUploadCommand {
                    scope: Self::drive_scope(context),
                    request: request.clone(),
                },
            )
            .await?;
        match self
            .repository
            .create_upload_session_ref(tenant_id, app_scope.as_deref(), request, &drive_response)
            .await
        {
            Ok(response) => Ok(response),
            Err(error @ sdkwork_deploy_contract::DeployServiceError::Conflict(_)) => {
                if let Some(existing) = self
                    .repository
                    .find_upload_session_by_idempotency_key(
                        tenant_id,
                        app_scope.as_deref(),
                        &request.idempotency_key,
                    )
                    .await?
                {
                    if Self::upload_session_request_matches_stored(request, &existing) {
                        return Ok(existing);
                    }
                }
                Err(error)
            }
            Err(error) => Err(error),
        }
    }

    async fn retrieve_upload_session(
        &self,
        context: &DeployAppRequestContext,
        upload_session_id: &str,
    ) -> DeployServiceResult<DeployUploadSessionResponse> {
        let tenant_id = Self::require_tenant(context)?;
        let app_scope = Self::caller_app_scope(context);
        let stored = self
            .repository
            .retrieve_upload_session_ref(tenant_id, app_scope, upload_session_id)
            .await?;
        let refreshed = self
            .drive
            .retrieve_upload_session(
                &Self::drive_credentials(context),
                &Self::drive_scope(context),
                &stored.drive_upload_session_id,
            )
            .await?;
        self.repository
            .update_upload_session_status(
                tenant_id,
                app_scope,
                upload_session_id,
                refreshed.status,
                refreshed.drive_node_id.as_deref(),
            )
            .await
    }

    async fn complete_upload_session(
        &self,
        context: &DeployAppRequestContext,
        upload_session_id: &str,
        request: &CompleteDeployUploadSessionRequest,
    ) -> DeployServiceResult<DeployUploadSessionResponse> {
        let tenant_id = Self::require_tenant(context)?;
        if request.checksum_sha256_hex.trim().is_empty() {
            return Err(sdkwork_deploy_contract::DeployServiceError::validation(
                "checksumSha256Hex is required",
            ));
        }
        let app_scope = Self::caller_app_scope(context);
        let stored = self
            .repository
            .retrieve_upload_session_ref(tenant_id, app_scope, upload_session_id)
            .await?;
        Self::ensure_upload_session_mutable(&stored)?;
        let drive_response = self
            .drive
            .complete_upload_session(
                &Self::drive_credentials(context),
                &Self::drive_scope(context),
                &stored.drive_upload_session_id,
                request,
            )
            .await?;
        let updated = self
            .repository
            .update_upload_session_status(
                tenant_id,
                app_scope,
                upload_session_id,
                drive_response.status,
                drive_response.drive_node_id.as_deref(),
            )
            .await?;
        if updated.status == UPLOAD_SESSION_STATUS_COMPLETED
            && is_deploy_package_artifact_type(updated.package_type)
        {
            self.repository
                .create_artifact_from_upload_session(
                    tenant_id,
                    app_scope,
                    upload_session_id,
                    &request.checksum_sha256_hex,
                )
                .await?;
        }
        Ok(updated)
    }

    async fn cancel_upload_session(
        &self,
        context: &DeployAppRequestContext,
        upload_session_id: &str,
    ) -> DeployServiceResult<DeployUploadSessionResponse> {
        let tenant_id = Self::require_tenant(context)?;
        let app_scope = Self::caller_app_scope(context);
        let stored = self
            .repository
            .retrieve_upload_session_ref(tenant_id, app_scope, upload_session_id)
            .await?;
        Self::ensure_upload_session_mutable(&stored)?;
        let drive_response = self
            .drive
            .cancel_upload_session(
                &Self::drive_credentials(context),
                &Self::drive_scope(context),
                &stored.drive_upload_session_id,
            )
            .await?;
        self.repository
            .update_upload_session_status(
                tenant_id,
                app_scope,
                upload_session_id,
                drive_response.status,
                drive_response.drive_node_id.as_deref(),
            )
            .await
    }

    // -- unified app delivery (REQ-2026-0002) ------------------------------------

    async fn list_apps(
        &self,
        context: &DeployAppRequestContext,
        query: &ListAppsQuery,
    ) -> DeployServiceResult<AppPage> {
        self.list_apps(context, query).await
    }

    async fn create_app(
        &self,
        context: &DeployAppRequestContext,
        idempotency_key: Option<&str>,
        request: &CreateAppRequest,
    ) -> DeployServiceResult<AppResponse> {
        self.create_app(context, idempotency_key, request).await
    }

    async fn retrieve_app(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
    ) -> DeployServiceResult<AppResponse> {
        self.retrieve_app(context, app_id).await
    }

    async fn update_app(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        request: &UpdateAppRequest,
    ) -> DeployServiceResult<AppResponse> {
        self.update_app(context, app_id, request).await
    }

    async fn list_app_domains(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
    ) -> DeployServiceResult<AppDomainPage> {
        self.list_app_domains(context, app_id).await
    }

    async fn create_platform_target(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        request: &CreatePlatformTargetRequest,
    ) -> DeployServiceResult<PlatformTargetResponse> {
        self.create_platform_target(context, app_id, request).await
    }

    async fn list_platform_targets(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
    ) -> DeployServiceResult<PlatformTargetPage> {
        self.list_platform_targets(context, app_id).await
    }

    async fn retrieve_platform_target(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        target_id: &str,
    ) -> DeployServiceResult<PlatformTargetResponse> {
        self.retrieve_platform_target(context, app_id, target_id)
            .await
    }

    async fn create_source_repository(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        request: &CreateSourceRepositoryRequest,
    ) -> DeployServiceResult<SourceRepositoryResponse> {
        self.create_source_repository(context, app_id, request)
            .await
    }

    async fn list_source_repositories(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
    ) -> DeployServiceResult<SourceRepositoryPage> {
        self.list_source_repositories(context, app_id).await
    }

    async fn retrieve_source_repository(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        repo_id: &str,
    ) -> DeployServiceResult<SourceRepositoryResponse> {
        self.retrieve_source_repository(context, app_id, repo_id)
            .await
    }

    async fn retrieve_app_source(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
    ) -> DeployServiceResult<AppSourceResponse> {
        self.retrieve_app_source(context, app_id).await
    }

    async fn update_app_source(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        request: &UpdateAppSourceRequest,
    ) -> DeployServiceResult<AppSourceResponse> {
        self.update_app_source(context, app_id, request).await
    }

    async fn delete_app_source(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
    ) -> DeployServiceResult<()> {
        self.delete_app_source(context, app_id).await
    }

    async fn validate_app_source(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
    ) -> DeployServiceResult<AppSourceResponse> {
        self.validate_app_source(context, app_id).await
    }

    async fn retrieve_app_remix(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
    ) -> DeployServiceResult<AppRemixResponse> {
        self.retrieve_app_remix(context, app_id).await
    }

    async fn update_app_remix(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        request: &UpdateAppRemixRequest,
    ) -> DeployServiceResult<AppRemixResponse> {
        self.update_app_remix(context, app_id, request).await
    }

    async fn create_build_template(
        &self,
        context: &DeployAppRequestContext,
        request: &CreateBuildTemplateRequest,
    ) -> DeployServiceResult<BuildTemplateResponse> {
        self.create_build_template(context, request).await
    }

    async fn list_build_templates(
        &self,
        context: &DeployAppRequestContext,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<BuildTemplatePage> {
        self.list_build_templates(context, page, page_size).await
    }

    async fn retrieve_build_template(
        &self,
        context: &DeployAppRequestContext,
        template_id: &str,
    ) -> DeployServiceResult<BuildTemplateResponse> {
        self.retrieve_build_template(context, template_id).await
    }

    async fn create_build(
        &self,
        context: &DeployAppRequestContext,
        request: &CreateBuildRequest,
    ) -> DeployServiceResult<BuildResponse> {
        self.create_build(context, request).await
    }

    async fn list_builds(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<BuildPage> {
        self.list_builds(context, app_id, page, page_size).await
    }

    async fn retrieve_build(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        build_id: &str,
    ) -> DeployServiceResult<BuildResponse> {
        self.retrieve_build(context, app_id, build_id).await
    }

    async fn update_build_state(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        build_id: &str,
        request: &UpdateBuildStateRequest,
    ) -> DeployServiceResult<BuildResponse> {
        self.update_build_state(context, app_id, build_id, request)
            .await
    }

    async fn register_package(
        &self,
        context: &DeployAppRequestContext,
        request: &RegisterPackageRequest,
    ) -> DeployServiceResult<PackageResponse> {
        self.register_package(context, request).await
    }

    async fn list_packages(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<PackagePage> {
        self.list_packages(context, app_id, page, page_size).await
    }

    async fn retrieve_package(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        package_id: &str,
    ) -> DeployServiceResult<PackageResponse> {
        self.retrieve_package(context, app_id, package_id).await
    }

    async fn create_app_release(
        &self,
        context: &DeployAppRequestContext,
        request: &CreateAppReleaseRequest,
    ) -> DeployServiceResult<AppReleaseResponse> {
        self.create_app_release(context, request).await
    }

    async fn list_app_releases(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<AppReleasePage> {
        self.list_app_releases(context, app_id, page, page_size)
            .await
    }

    async fn list_channel_releases(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        environment: Option<&str>,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<sdkwork_deploy_contract::ChannelReleasePage> {
        let tenant_id = Self::tenant_id(context)?;
        self.assert_app_access(context, tenant_id, app_id).await?;
        if let Some(environment) = environment {
            if !matches!(
                environment,
                "development" | "test" | "staging" | "demo" | "production"
            ) {
                return Err(DeployServiceError::validation(
                    "environment must be development, test, staging, demo, or production",
                ));
            }
        }
        self.repository
            .list_channel_releases(tenant_id, app_id, environment, page, page_size)
            .await
    }

    async fn retrieve_channel_release(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        release_id: &str,
    ) -> DeployServiceResult<sdkwork_deploy_contract::ChannelReleaseResponse> {
        let tenant_id = Self::tenant_id(context)?;
        self.assert_app_access(context, tenant_id, app_id).await?;
        self.repository
            .retrieve_channel_release(tenant_id, app_id, release_id)
            .await
    }

    async fn create_channel_release(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        request: &sdkwork_deploy_contract::CreateChannelReleaseRequest,
    ) -> DeployServiceResult<sdkwork_deploy_contract::ChannelReleaseResponse> {
        let tenant_id = Self::tenant_id(context)?;
        self.assert_app_access(context, tenant_id, app_id).await?;
        if !matches!(
            request.environment.as_str(),
            "development" | "test" | "staging" | "demo" | "production"
        ) {
            return Err(DeployServiceError::validation(
                "environment must be development, test, staging, demo, or production",
            ));
        }
        self.enforce_entitlement(
            tenant_id,
            sdkwork_deploy_contract::ENTITLEMENT_DIMENSION_CHANNEL_RELEASE_COUNT,
        )
        .await?;
        let release = self
            .repository
            .create_channel_release(tenant_id, context.actor_id, app_id, request)
            .await?;
        self.audit_app_action(context, "channel_release.create", &release.id)
            .await?;
        // The release id is the dedup key, so an idempotent replay never
        // over-counts the publish operation.
        self.record_usage(crate::repository::InsertUsageEventCommand {
            tenant_id: context.tenant_id,
            organization_id: context.organization_id.unwrap_or(0),
            app_id: None,
            binding_id: None,
            attribution: None,
            period_start: Self::billing_period_start(None),
            dimension: sdkwork_deploy_contract::USAGE_DIMENSION_CHANNEL_RELEASE_COUNT.to_owned(),
            quantity: 1,
            unit: "COUNT".to_owned(),
            source_target_uuid: Some(release.app_id.clone()),
            source_window_id: Some(format!("channel_release:{}", release.id)),
            deduplication_key: format!("channel_release:{}", release.id),
        })
        .await;
        Ok(release)
    }

    async fn transition_channel_release(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        release_id: &str,
        request: &sdkwork_deploy_contract::TransitionChannelReleaseRequest,
    ) -> DeployServiceResult<sdkwork_deploy_contract::ChannelReleaseResponse> {
        let tenant_id = Self::tenant_id(context)?;
        self.assert_app_access(context, tenant_id, app_id).await?;
        self.repository
            .transition_channel_release(tenant_id, app_id, release_id, request)
            .await
    }

    async fn publish_channel_release(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        release_id: &str,
        request: &sdkwork_deploy_contract::ChannelReleasePublishCommandRequest,
    ) -> DeployServiceResult<sdkwork_deploy_contract::ChannelReleaseResponse> {
        self.publish_channel_release(context, app_id, release_id, request)
            .await
    }

    async fn plan_channel_direct_upload(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        request: &sdkwork_deploy_contract::CreateChannelDirectUploadPlanRequest,
    ) -> DeployServiceResult<sdkwork_deploy_contract::ChannelDirectUploadPlan> {
        self.plan_channel_direct_upload(context, app_id, request)
            .await
    }

    async fn retrieve_app_release(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        release_id: &str,
    ) -> DeployServiceResult<AppReleaseResponse> {
        self.retrieve_app_release(context, app_id, release_id).await
    }

    async fn update_app_release_status(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        release_id: &str,
        release_status: ReleaseStatus,
    ) -> DeployServiceResult<AppReleaseResponse> {
        self.update_app_release_status(context, app_id, release_id, release_status)
            .await
    }

    async fn list_channels(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
    ) -> DeployServiceResult<ChannelPage> {
        self.list_channels(context, app_id).await
    }

    async fn retrieve_channel(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        channel_id: &str,
    ) -> DeployServiceResult<ChannelResponse> {
        self.retrieve_channel(context, app_id, channel_id).await
    }

    async fn promote_channel(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        channel_id: &str,
        request: &PromoteChannelRequest,
    ) -> DeployServiceResult<ChannelRolloutResponse> {
        self.promote_channel(context, app_id, channel_id, request)
            .await
    }

    async fn list_channel_rollouts(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        channel_id: &str,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<ChannelRolloutPage> {
        self.list_channel_rollouts(context, app_id, channel_id, page, page_size)
            .await
    }

    async fn create_app_deployment(
        &self,
        context: &DeployAppRequestContext,
        request: &CreateAppDeploymentRequest,
    ) -> DeployServiceResult<AppDeploymentResponse> {
        self.create_app_deployment(context, request).await
    }

    async fn list_app_deployments(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<AppDeploymentPage> {
        self.list_app_deployments(context, app_id, page, page_size)
            .await
    }

    async fn retrieve_app_deployment(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        deployment_id: &str,
    ) -> DeployServiceResult<AppDeploymentResponse> {
        self.retrieve_app_deployment(context, app_id, deployment_id)
            .await
    }

    async fn create_signing_identity(
        &self,
        context: &DeployAppRequestContext,
        request: &CreateSigningIdentityRequest,
    ) -> DeployServiceResult<SigningIdentityResponse> {
        self.create_signing_identity(context, request).await
    }

    async fn list_signing_identities(
        &self,
        context: &DeployAppRequestContext,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<SigningIdentityPage> {
        self.list_signing_identities(context, page, page_size).await
    }

    async fn retrieve_signing_identity(
        &self,
        context: &DeployAppRequestContext,
        identity_id: &str,
    ) -> DeployServiceResult<SigningIdentityResponse> {
        self.retrieve_signing_identity(context, identity_id).await
    }

    async fn list_usage_events(
        &self,
        context: &DeployAppRequestContext,
        query: &UsageEventQuery,
    ) -> DeployServiceResult<UsageEventPage> {
        self.list_usage_events(context, query).await
    }

    async fn create_app_database_profile(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        request: &CreateAppDatabaseProfileRequest,
    ) -> DeployServiceResult<AppDatabaseProfileResponse> {
        self.create_app_database_profile(context, app_id, request)
            .await
    }

    async fn list_app_database_profiles(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<AppDatabaseProfilePage> {
        self.list_app_database_profiles(context, app_id, page, page_size)
            .await
    }

    async fn retrieve_app_database_profile(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        profile_id: &str,
    ) -> DeployServiceResult<AppDatabaseProfileResponse> {
        self.retrieve_app_database_profile(context, app_id, profile_id)
            .await
    }

    async fn update_app_database_profile(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        profile_id: &str,
        request: &UpdateAppDatabaseProfileRequest,
    ) -> DeployServiceResult<AppDatabaseProfileResponse> {
        self.update_app_database_profile(context, app_id, profile_id, request)
            .await
    }

    async fn create_app_database_migration(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        profile_id: &str,
        request: &CreateAppDatabaseMigrationRequest,
    ) -> DeployServiceResult<AppDatabaseMigrationResponse> {
        self.create_app_database_migration(context, app_id, profile_id, request)
            .await
    }

    async fn list_app_database_migrations(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        profile_id: &str,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<AppDatabaseMigrationPage> {
        self.list_app_database_migrations(context, app_id, profile_id, page, page_size)
            .await
    }

    async fn retrieve_app_database_migration(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        profile_id: &str,
        migration_id: &str,
    ) -> DeployServiceResult<AppDatabaseMigrationResponse> {
        self.retrieve_app_database_migration(context, app_id, profile_id, migration_id)
            .await
    }

    async fn create_app_environment(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        request: &CreateAppEnvironmentRequest,
    ) -> DeployServiceResult<AppEnvironmentResponse> {
        self.create_app_environment(context, app_id, request).await
    }

    async fn list_app_environments(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<AppEnvironmentPage> {
        self.list_app_environments(context, app_id, page, page_size)
            .await
    }

    async fn retrieve_app_environment(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        environment_id: &str,
    ) -> DeployServiceResult<AppEnvironmentResponse> {
        self.retrieve_app_environment(context, app_id, environment_id)
            .await
    }

    async fn update_app_environment(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        environment_id: &str,
        request: &UpdateAppEnvironmentRequest,
    ) -> DeployServiceResult<AppEnvironmentResponse> {
        self.update_app_environment(context, app_id, environment_id, request)
            .await
    }

    async fn promote_environment(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        environment_id: &str,
        request: &PromoteEnvironmentRequest,
    ) -> DeployServiceResult<EnvironmentPromotionResponse> {
        self.promote_environment(context, app_id, environment_id, request)
            .await
    }

    async fn list_environment_promotions(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        environment_id: &str,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<EnvironmentPromotionPage> {
        self.list_environment_promotions(context, app_id, environment_id, page, page_size)
            .await
    }

    // -- source specs ---------------------------------------------------------

    async fn list_app_source_specs(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        environment: Option<&str>,
    ) -> DeployServiceResult<AppSourceSpecPage> {
        self.list_app_source_specs(context, app_id, environment)
            .await
    }

    async fn create_app_source_spec(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        idempotency_key: &str,
        request: &CreateAppSourceSpecRequest,
    ) -> DeployServiceResult<AppSourceSpecResponse> {
        self.create_app_source_spec(context, app_id, idempotency_key, request)
            .await
    }

    async fn retrieve_app_source_spec(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        spec_id: &str,
    ) -> DeployServiceResult<AppSourceSpecResponse> {
        self.retrieve_app_source_spec(context, app_id, spec_id)
            .await
    }

    async fn update_app_source_spec(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        spec_id: &str,
        expected_spec_version: i64,
        request: &UpdateAppSourceSpecRequest,
    ) -> DeployServiceResult<AppSourceSpecResponse> {
        self.update_app_source_spec(context, app_id, spec_id, expected_spec_version, request)
            .await
    }

    async fn delete_app_source_spec(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        spec_id: &str,
    ) -> DeployServiceResult<()> {
        self.delete_app_source_spec(context, app_id, spec_id).await
    }

    async fn bind_app_source_spec_source(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        spec_id: &str,
        request: &BindAppSourceSpecSourceRequest,
    ) -> DeployServiceResult<AppSourceSpecResponse> {
        self.bind_app_source_spec_source(context, app_id, spec_id, request)
            .await
    }

    // -- app scaffold marketplace ---------------------------------------------

    async fn list_scaffold_categories(
        &self,
        context: &DeployAppRequestContext,
        include_disabled: bool,
    ) -> DeployServiceResult<sdkwork_deploy_contract::ScaffoldCategoryPage> {
        let tenant_id = Self::require_tenant(context)?;
        self.repository
            .list_scaffold_categories(Some(tenant_id), include_disabled, 1, 200)
            .await
    }

    async fn list_marketplace_scaffolds(
        &self,
        context: &DeployAppRequestContext,
        query: &sdkwork_deploy_contract::ListMarketplaceScaffoldsQuery,
    ) -> DeployServiceResult<sdkwork_deploy_contract::ApplicationScaffoldSummaryPage> {
        let tenant_id = Self::require_tenant(context)?;
        if let Some(pricing) = query.pricing_model.as_deref() {
            if pricing != sdkwork_deploy_contract::SCAFFOLD_PRICING_FREE
                && pricing != sdkwork_deploy_contract::SCAFFOLD_PRICING_PAID
            {
                return Err(sdkwork_deploy_contract::DeployServiceError::validation(
                    "pricingModel is invalid",
                ));
            }
        }
        if let Some(template_type) = query.template_type.as_deref() {
            if template_type != sdkwork_deploy_contract::SCAFFOLD_TYPE_APP
                && template_type != sdkwork_deploy_contract::SCAFFOLD_TYPE_PPT
                && template_type != sdkwork_deploy_contract::SCAFFOLD_TYPE_VIDEO
            {
                return Err(sdkwork_deploy_contract::DeployServiceError::validation(
                    "templateType is invalid",
                ));
            }
        }
        if let Some(sort) = query.sort.as_deref() {
            if sort != sdkwork_deploy_contract::MARKETPLACE_SORT_NEWEST
                && sort != sdkwork_deploy_contract::MARKETPLACE_SORT_POPULAR
            {
                return Err(sdkwork_deploy_contract::DeployServiceError::validation(
                    "sort is invalid",
                ));
            }
        }
        self.repository
            .list_marketplace_scaffolds(tenant_id, query)
            .await
    }

    async fn retrieve_marketplace_scaffold(
        &self,
        context: &DeployAppRequestContext,
        scaffold_uuid: &str,
    ) -> DeployServiceResult<sdkwork_deploy_contract::ApplicationScaffoldResponse> {
        let tenant_id = Self::require_tenant(context)?;
        self.repository
            .retrieve_marketplace_scaffold(tenant_id, scaffold_uuid)
            .await
    }

    async fn install_application_scaffold(
        &self,
        context: &DeployAppRequestContext,
        scaffold_uuid: &str,
        idempotency_key: &str,
        request: &sdkwork_deploy_contract::InstallApplicationScaffoldRequest,
    ) -> DeployServiceResult<sdkwork_deploy_contract::ApplicationScaffoldInstallResponse> {
        DeployService::install_application_scaffold(
            self,
            context,
            scaffold_uuid,
            idempotency_key,
            request,
        )
        .await
    }

    async fn list_application_scaffolds(
        &self,
        context: &DeployAppRequestContext,
        query: &sdkwork_deploy_contract::ListApplicationScaffoldsQuery,
    ) -> DeployServiceResult<sdkwork_deploy_contract::ApplicationScaffoldPage> {
        let tenant_id = Self::require_tenant(context)?;
        let author_user_id = Self::require_actor(context)?;
        if let Some(status) = query.status.as_deref() {
            validate_scaffold_status(status)?;
        }
        self.repository
            .list_application_scaffolds(tenant_id, Some(author_user_id), query)
            .await
    }

    async fn create_application_scaffold(
        &self,
        context: &DeployAppRequestContext,
        _idempotency_key: &str,
        request: &sdkwork_deploy_contract::CreateApplicationScaffoldRequest,
    ) -> DeployServiceResult<sdkwork_deploy_contract::ApplicationScaffoldResponse> {
        let tenant_id = Self::require_tenant(context)?;
        let author_user_id = Self::require_actor(context)?;
        self.repository
            .create_application_scaffold(
                tenant_id,
                context.organization_id,
                Some(author_user_id),
                request,
            )
            .await
    }

    async fn retrieve_application_scaffold(
        &self,
        context: &DeployAppRequestContext,
        scaffold_uuid: &str,
    ) -> DeployServiceResult<sdkwork_deploy_contract::ApplicationScaffoldResponse> {
        let tenant_id = Self::require_tenant(context)?;
        let author_user_id = Self::require_actor(context)?;
        match self
            .resolve_scaffold_read_access(tenant_id, author_user_id, scaffold_uuid)
            .await?
        {
            ScaffoldReadAccess::Author => {
                self.repository
                    .retrieve_application_scaffold(
                        Some(tenant_id),
                        Some(author_user_id),
                        scaffold_uuid,
                    )
                    .await
            }
            // A consumer installing someone else's scaffold reads the same
            // listing the marketplace shelf advertises, never the author view.
            ScaffoldReadAccess::Marketplace => {
                self.repository
                    .retrieve_marketplace_scaffold(tenant_id, scaffold_uuid)
                    .await
            }
            ScaffoldReadAccess::Denied => Err(DeployServiceError::not_found("scaffold not found")),
        }
    }

    async fn update_application_scaffold(
        &self,
        context: &DeployAppRequestContext,
        scaffold_uuid: &str,
        request: &sdkwork_deploy_contract::UpdateApplicationScaffoldRequest,
    ) -> DeployServiceResult<sdkwork_deploy_contract::ApplicationScaffoldResponse> {
        let tenant_id = Self::require_tenant(context)?;
        let author_user_id = Self::require_actor(context)?;
        self.repository
            .update_application_scaffold(tenant_id, author_user_id, scaffold_uuid, request)
            .await
    }

    async fn delete_application_scaffold(
        &self,
        context: &DeployAppRequestContext,
        scaffold_uuid: &str,
    ) -> DeployServiceResult<()> {
        let tenant_id = Self::require_tenant(context)?;
        let author_user_id = Self::require_actor(context)?;
        self.repository
            .delete_application_scaffold(Some(tenant_id), Some(author_user_id), scaffold_uuid)
            .await
    }

    async fn submit_application_scaffold(
        &self,
        context: &DeployAppRequestContext,
        scaffold_uuid: &str,
    ) -> DeployServiceResult<sdkwork_deploy_contract::ApplicationScaffoldResponse> {
        let tenant_id = Self::require_tenant(context)?;
        let author_user_id = Self::require_actor(context)?;
        self.repository
            .submit_application_scaffold(tenant_id, author_user_id, scaffold_uuid)
            .await
    }

    async fn list_application_scaffold_versions(
        &self,
        context: &DeployAppRequestContext,
        scaffold_uuid: &str,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<sdkwork_deploy_contract::ApplicationScaffoldVersionPage> {
        let tenant_id = Self::require_tenant(context)?;
        let author_user_id = Self::require_actor(context)?;
        match self
            .resolve_scaffold_read_access(tenant_id, author_user_id, scaffold_uuid)
            .await?
        {
            ScaffoldReadAccess::Author => {
                self.repository
                    .list_application_scaffold_versions(
                        Some(tenant_id),
                        Some(author_user_id),
                        scaffold_uuid,
                        page,
                        page_size,
                    )
                    .await
            }
            // The install flow of a marketplace consumer: published versions of
            // a published, public listing. Drafts stay author-only.
            ScaffoldReadAccess::Marketplace => {
                self.repository
                    .list_marketplace_scaffold_versions(tenant_id, scaffold_uuid, page, page_size)
                    .await
            }
            ScaffoldReadAccess::Denied => Err(DeployServiceError::not_found("scaffold not found")),
        }
    }

    async fn create_application_scaffold_version(
        &self,
        context: &DeployAppRequestContext,
        scaffold_uuid: &str,
        _idempotency_key: &str,
        request: &sdkwork_deploy_contract::CreateApplicationScaffoldVersionRequest,
    ) -> DeployServiceResult<sdkwork_deploy_contract::ApplicationScaffoldVersionResponse> {
        let tenant_id = Self::require_tenant(context)?;
        let author_user_id = Self::require_actor(context)?;
        self.repository
            .create_application_scaffold_version(tenant_id, author_user_id, scaffold_uuid, request)
            .await
    }
}

/// Author surfaces are user-private (`IAM_SPEC.md` 5.1): a caller without a
/// user subject has no listings of their own, so the answer is "forbidden"
/// rather than an anonymous tenant-wide view.
impl DeployService {
    pub(crate) fn require_actor(context: &DeployAppRequestContext) -> DeployServiceResult<i64> {
        context
            .actor_id
            .filter(|actor_id| *actor_id > 0)
            .ok_or_else(|| {
                sdkwork_deploy_contract::DeployServiceError::forbidden(
                    "a user subject is required for scaffold marketplace operations",
                )
            })
    }

    /// Resolves which read an install-flow caller may take on one listing.
    ///
    /// The lookup carries no listing body, and the answer is chosen before one
    /// is read: the author keeps the author-scope read (drafts included), a
    /// consumer is routed to the marketplace-visible published read, and
    /// everyone else gets the same "not found" a missing listing produces, so a
    /// draft's existence never leaks.
    async fn resolve_scaffold_read_access(
        &self,
        tenant_id: i64,
        actor_id: i64,
        scaffold_uuid: &str,
    ) -> DeployServiceResult<ScaffoldReadAccess> {
        let Some(scope) = self
            .repository
            .scaffold_read_scope(tenant_id, scaffold_uuid)
            .await?
        else {
            return Ok(ScaffoldReadAccess::Denied);
        };
        Ok(scaffold_read_access(
            actor_id,
            scope.author_user_id,
            &scope.visibility,
            &scope.status,
        ))
    }
}

fn validate_scaffold_status(status: &str) -> DeployServiceResult<()> {
    let valid = matches!(
        status,
        sdkwork_deploy_contract::SCAFFOLD_STATUS_DRAFT
            | sdkwork_deploy_contract::SCAFFOLD_STATUS_PENDING_REVIEW
            | sdkwork_deploy_contract::SCAFFOLD_STATUS_PUBLISHED
            | sdkwork_deploy_contract::SCAFFOLD_STATUS_REJECTED
            | sdkwork_deploy_contract::SCAFFOLD_STATUS_DISABLED
    );
    if valid {
        Ok(())
    } else {
        Err(sdkwork_deploy_contract::DeployServiceError::validation(
            "scaffold status is invalid",
        ))
    }
}

#[cfg(test)]
mod upload_session_tests {
    use super::*;
    use sdkwork_deploy_contract::CreateDeployUploadSessionRequest;

    fn sample_request() -> CreateDeployUploadSessionRequest {
        CreateDeployUploadSessionRequest {
            app_id: None,
            package_type: 1,
            file_name: "app.zip".to_string(),
            content_type: "application/zip".to_string(),
            content_length: 1024,
            checksum: None,
            idempotency_key: "idem-1".to_string(),
        }
    }

    #[test]
    fn validate_upload_request_rejects_invalid_package_type() {
        let mut request = sample_request();
        request.package_type = 9;
        assert!(DeployService::validate_upload_request(&request).is_err());
    }

    #[test]
    fn upload_session_request_matches_stored_compares_payload_fields() {
        let request = sample_request();
        let stored = DeployUploadSessionResponse {
            id: "sess-1".to_string(),
            app_id: None,
            package_type: 1,
            file_name: "app.zip".to_string(),
            content_type: "application/zip".to_string(),
            content_length: 1024,
            checksum: None,
            status: 0,
            drive_upload_session_id: "drive-1".to_string(),
            drive_upload_item_id: None,
            drive_space_id: None,
            drive_node_id: None,
            created_at: "2026-01-01T00:00:00.000Z".to_string(),
            updated_at: "2026-01-01T00:00:00.000Z".to_string(),
        };
        assert!(DeployService::upload_session_request_matches_stored(
            &request, &stored
        ));
    }

    #[test]
    fn ensure_upload_session_mutable_rejects_terminal_states() {
        let completed = DeployUploadSessionResponse {
            id: "sess-1".to_string(),
            app_id: None,
            package_type: 1,
            file_name: "app.zip".to_string(),
            content_type: "application/zip".to_string(),
            content_length: 1024,
            checksum: None,
            status: UPLOAD_SESSION_STATUS_COMPLETED,
            drive_upload_session_id: "drive-1".to_string(),
            drive_upload_item_id: None,
            drive_space_id: None,
            drive_node_id: None,
            created_at: String::new(),
            updated_at: String::new(),
        };
        assert!(DeployService::ensure_upload_session_mutable(&completed).is_err());
    }
}

#[cfg(test)]
mod app_scope_tests {
    use super::*;

    fn context_with_app(app_id: Option<&str>) -> DeployAppRequestContext {
        DeployAppRequestContext {
            tenant_id: 7,
            actor_id: Some(11),
            organization_id: Some(9),
            session_id: Some("session-1".to_owned()),
            app_id: app_id.map(str::to_owned),
            credential_kind: sdkwork_deploy_contract::DeployCredentialKind::Application,
            auth_token: None,
            access_token: None,
            permissions: Vec::new(),
        }
    }

    /// The console is an application too, so its operator's session proves the
    /// console's `app_id` while the page manages other applications. That claim is
    /// not a data-access binding: binding it answered `40301` for every app on the
    /// page. The tenant/owner gate decides instead.
    #[test]
    fn user_session_app_claim_does_not_bind_data_access() {
        let console = DeployAppRequestContext {
            credential_kind: sdkwork_deploy_contract::DeployCredentialKind::UserSession,
            ..context_with_app(Some("381764687714324480"))
        };
        assert_eq!(console.app_scope(), Some("381764687714324480"));
        assert_eq!(DeployService::caller_app_scope(&console), None);
        assert!(
            DeployService::assert_caller_app_scope(&console, "another-application").is_ok(),
            "a console session must reach the owner gate, not be refused here"
        );
    }

    /// `APPLICATION_IDENTITY_SPEC.md` §4.3: the data-plane filter follows the
    /// deploy application the *credential* proves. Application A's token can
    /// therefore only ever yield A's scope — never B's — which is what keeps
    /// `list_artifacts` / `retrieve_artifact` / the upload-session family from
    /// answering with another application's rows.
    #[test]
    fn token_app_scope_is_the_credentials_application_only() {
        let app_a = context_with_app(Some("app-a"));
        assert_eq!(DeployService::caller_app_scope(&app_a), Some("app-a"));
        assert_ne!(DeployService::caller_app_scope(&app_a), Some("app-b"));
    }

    /// A blank claim names no application. It must read as "no scope" so the
    /// query keeps the pre-existing tenant/owner boundary instead of building a
    /// filter that matches no row while appearing to be scoped.
    #[test]
    fn blank_or_absent_app_claim_reads_as_no_scope() {
        for app_id in [None, Some(""), Some("   ")] {
            let context = context_with_app(app_id);
            assert_eq!(
                DeployService::caller_app_scope(&context),
                None,
                "{app_id:?} must not be read as an application identity"
            );
        }
    }

    /// A write may only name the application the credential proves. Naming B
    /// from A's credential is refused outright rather than silently rewritten:
    /// the response would otherwise describe a row the caller never asked for.
    #[test]
    fn write_naming_another_application_is_refused() {
        let context = context_with_app(Some("app-a"));
        let error = DeployService::bind_caller_app_scope(&context, Some("app-b"))
            .expect_err("a write bound to app-b must be refused for app-a's credential");
        assert_eq!(error.kind(), DeployServiceErrorKind::Forbidden);
    }

    /// The same application is accepted, and an omitted body `appId` binds the
    /// row to the credential's application so the caller can still read it back.
    #[test]
    fn write_binds_to_the_credentials_application() {
        let context = context_with_app(Some("app-a"));
        assert_eq!(
            DeployService::bind_caller_app_scope(&context, None).expect("own app, implicit"),
            Some("app-a".to_owned())
        );
        assert_eq!(
            DeployService::bind_caller_app_scope(&context, Some(" app-a "))
                .expect("own app, explicit"),
            Some("app-a".to_owned())
        );
    }

    /// A credential that names no application keeps today's behaviour: the body
    /// decides the attribution, and an omitted one stays un-attributed. This is
    /// the compatibility side of the change — no existing caller is narrowed.
    #[test]
    fn credential_without_application_keeps_the_requested_binding() {
        let unscoped = context_with_app(None);
        assert_eq!(
            DeployService::bind_caller_app_scope(&unscoped, Some("app-b")).expect("tenant caller"),
            Some("app-b".to_owned())
        );
        assert_eq!(
            DeployService::bind_caller_app_scope(&unscoped, None).expect("tenant caller"),
            None
        );
    }

    /// The claim is the deploy application anchor (`deploy_app.id`, decimal); the
    /// wire address is the uuid. The path gate cannot compare the two forms
    /// without the row, so it carries the value through and the row gate
    /// (`assert_app_reachable`) decides. Refusing here would refuse an
    /// application its own data.
    #[test]
    fn path_gate_defers_the_anchor_versus_uuid_identity_form() {
        let machine = context_with_app(Some("381764687714324480"));
        assert!(
            DeployService::assert_caller_app_scope(
                &machine,
                "ef45ca0e-0093-4eaf-ab03-964d8ef6227c"
            )
            .is_ok(),
            "the decimal anchor and the uuid are the same identity in two forms"
        );
        assert_eq!(
            DeployService::bind_caller_app_scope(
                &machine,
                Some("ef45ca0e-0093-4eaf-ab03-964d8ef6227c")
            )
            .expect("the row gate decides the cross-form case"),
            Some("ef45ca0e-0093-4eaf-ab03-964d8ef6227c".to_owned())
        );
    }

    /// Same-form identities remain comparable without I/O: two anchors, two
    /// uuids, and arbitrary identifiers are compared directly.
    #[test]
    fn identity_forms_are_only_deferred_when_anchor_meets_uuid() {
        assert!(app_identity_forms_differ(
            "381764687714324480",
            "ef45ca0e-0093-4eaf-ab03-964d8ef6227c"
        ));
        assert!(app_identity_forms_differ(
            "ef45ca0e-0093-4eaf-ab03-964d8ef6227c",
            "381764687714324480"
        ));
        assert!(!app_identity_forms_differ(
            "381764687714324480",
            "382429273660592128"
        ));
        assert!(!app_identity_forms_differ(
            "ef45ca0e-0093-4eaf-ab03-964d8ef6227c",
            "10e8c045-f2fc-41c4-bdfe-48fb2344a9a6"
        ));
        assert!(!app_identity_forms_differ("app-a", "app-b"));
    }

    /// A **path-addressed** application is checked against the credential exactly
    /// like a body field: A may address A (whitespace tolerated), never B. This is
    /// the gate `health_checks` / `env_variables` / `app_composition` now run
    /// before their owner gate.
    #[test]
    fn addressing_another_application_in_the_path_is_refused() {
        let app_a = context_with_app(Some("app-a"));
        assert!(
            DeployService::assert_caller_app_scope(&app_a, "app-a").is_ok(),
            "the credential's own application must stay addressable"
        );
        assert!(
            DeployService::assert_caller_app_scope(&app_a, " app-a ").is_ok(),
            "a padded path segment names the same application"
        );
        let error = DeployService::assert_caller_app_scope(&app_a, "app-b")
            .expect_err("a path naming app-b must be refused for app-a's credential");
        assert_eq!(error.kind(), DeployServiceErrorKind::Forbidden);
    }

    /// The scope gate must not become a blanket denial: with no application
    /// identity in the credential the tenant/owner gate decides, so a console or
    /// user-auth caller keeps working.
    #[test]
    fn unscoped_credential_defers_the_path_check_to_the_owner_gate() {
        let unscoped = context_with_app(None);
        assert!(
            DeployService::assert_caller_app_scope(&unscoped, "app-b").is_ok(),
            "without an app identity the owner gate is the boundary, not this check"
        );
    }
}

#[cfg(test)]
mod domain_zone_tests {
    use super::normalize_relative_hostname;

    #[test]
    fn relative_names_support_apex_multi_level_and_wildcard() {
        assert_eq!(
            normalize_relative_hostname("@", "example.com").unwrap(),
            "@"
        );
        assert_eq!(
            normalize_relative_hostname("WWW", "example.com").unwrap(),
            "www"
        );
        assert_eq!(
            normalize_relative_hostname("api.eu", "example.com").unwrap(),
            "api.eu"
        );
        assert_eq!(
            normalize_relative_hostname("*", "example.com").unwrap(),
            "*"
        );
        assert_eq!(
            normalize_relative_hostname("*.a", "example.com").unwrap(),
            "*.a"
        );
    }

    #[test]
    fn relative_names_stay_inside_the_zone() {
        for name in [
            "", ".", "@.x", "a..b", "a.b.", "foo.*", "*x", "*.a.*", "*.x..y",
        ] {
            assert!(
                normalize_relative_hostname(name, "example.com").is_err(),
                "{name}"
            );
        }
    }
}

// -- unified app delivery (REQ-2026-0002) ------------------------------------

impl DeployService {
    pub async fn list_apps_api(
        &self,
        context: &DeployAppRequestContext,
        query: &ListAppsQuery,
    ) -> DeployServiceResult<AppPage> {
        self.list_apps(context, query).await
    }
}
