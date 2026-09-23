//! App-api service surface implementation.

use async_trait::async_trait;
use sdkwork_deploy_contract::{
    is_deploy_package_artifact_type, AppDatabaseMigrationPage, AppDatabaseMigrationResponse,
    AppDatabaseProfilePage, AppDatabaseProfileResponse, AppDeploymentPage, AppDeploymentResponse,
    AppDomainPage, AppEnvironmentPage, AppEnvironmentResponse, AppPage, AppReleasePage,
    AppReleaseResponse, AppResponse, BuildPage, BuildResponse, BuildTemplatePage,
    BuildTemplateResponse, ChannelPage, ChannelResponse, ChannelRolloutPage,
    ChannelRolloutResponse, CompleteDeployUploadSessionRequest, CreateAppDatabaseMigrationRequest,
    CreateAppDatabaseProfileRequest, CreateAppDeploymentRequest, CreateAppEnvironmentRequest,
    CreateAppReleaseRequest, CreateAppRequest, CreateArtifactRequest, CreateBuildRequest,
    CreateBuildTemplateRequest, CreateCertificateRequest, CreateDeployUploadSessionRequest,
    CreateDomainHostnameRequest, CreateDomainZoneRequest, CreateEnvVariableRequest,
    CreateHealthCheckRequest, CreatePlatformTargetRequest, CreateSigningIdentityRequest,
    CreateSourceRepositoryRequest, DeployAppApi, DeployAppRequestContext, DeployServiceResult,
    DeployUploadSessionResponse, EnvironmentPromotionPage, EnvironmentPromotionResponse,
    ListAppsQuery, ListDomainZonesQuery, PackagePage, PackageResponse, PlatformTargetPage,
    PlatformTargetResponse, PromoteChannelRequest, PromoteEnvironmentRequest,
    RegisterPackageRequest, ReleaseStatus, RequestCertificateOrderRequest, SigningIdentityPage,
    SigningIdentityResponse, SourceRepositoryPage, SourceRepositoryResponse,
    UpdateAppDatabaseProfileRequest, UpdateAppEnvironmentRequest, UpdateAppRequest,
    UpdateBuildStateRequest, UpdateDomainHostnameRequest, UpdateDomainZoneRequest, UsageEventPage,
    UsageEventQuery, UPLOAD_SESSION_STATUS_CANCELLED, UPLOAD_SESSION_STATUS_COMPLETED,
};
use sdkwork_deploy_drive_port::{DriveRequestCredentials, PrepareDeployUploadCommand};

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
        owner_user_id: Option<i64>,
        zone_id: &str,
        hostname_id: &str,
    ) -> DeployServiceResult<sdkwork_deploy_contract::DomainVerifyResponse> {
        let challenge = self
            .repository
            .domain_hostname_verification_challenge(tenant_id, owner_user_id, zone_id, hostname_id)
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
                owner_user_id,
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
            .domain_hostname_verification_challenge(tenant_id, owner_user_id, zone_id, hostname_id)
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
            .list_domain_zones(tenant_id, context.actor_id, query)
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
            .retrieve_domain_zone(tenant_id, context.actor_id, zone_id)
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
            .update_domain_zone(tenant_id, context.actor_id, zone_id, &request)
            .await
    }

    async fn delete_domain_zone(
        &self,
        context: &DeployAppRequestContext,
        zone_id: &str,
    ) -> DeployServiceResult<()> {
        let tenant_id = Self::require_tenant(context)?;
        self.repository
            .delete_domain_zone(tenant_id, context.actor_id, zone_id)
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
            .list_domain_hostnames(tenant_id, context.actor_id, zone_id, page, page_size)
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
            .retrieve_domain_zone(tenant_id, context.actor_id, zone_id)
            .await?;
        let relative_name =
            normalize_relative_hostname(&request.relative_name, &zone.apex_hostname)?;
        self.repository
            .create_domain_hostname(
                tenant_id,
                context.actor_id,
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
            .retrieve_domain_hostname(tenant_id, context.actor_id, zone_id, hostname_id)
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
            .delete_domain_hostname(tenant_id, context.actor_id, zone_id, hostname_id)
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
            .retrieve_domain_zone(tenant_id, context.actor_id, zone_id)
            .await?;
        let relative_name =
            normalize_relative_hostname(&request.relative_name, &zone.apex_hostname)?;
        self.repository
            .update_domain_hostname(
                tenant_id,
                context.actor_id,
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
        self.advance_hostname_ownership(tenant_id, context.actor_id, zone_id, hostname_id)
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
            .retrieve_domain_zone(tenant_id, context.actor_id, zone_id)
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
                .ensure_domain_hostname(tenant_id, context.actor_id, zone_id, relative_name)
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
                        context.actor_id,
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
                vendor_code: dns_provider
                    .and_then(dns_provider::vendor_code_for)
                    .map(str::to_owned),
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
        let app = self.repository.set_app_status(tenant_id, app_id, 2).await?;
        let _ = self.audit_app_action(context, "app.pause", app_id).await;
        Ok(app)
    }

    async fn list_artifacts(
        &self,
        context: &DeployAppRequestContext,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<sdkwork_deploy_contract::ArtifactPage> {
        let tenant_id = Self::require_tenant(context)?;
        self.repository
            .list_artifacts(tenant_id, page, page_size)
            .await
    }

    async fn create_artifact(
        &self,
        context: &DeployAppRequestContext,
        request: &CreateArtifactRequest,
    ) -> DeployServiceResult<sdkwork_deploy_contract::ArtifactResponse> {
        let tenant_id = Self::require_tenant(context)?;
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
            .create_artifact_from_drive(tenant_id, request)
            .await
    }

    async fn retrieve_artifact(
        &self,
        context: &DeployAppRequestContext,
        artifact_id: &str,
    ) -> DeployServiceResult<sdkwork_deploy_contract::ArtifactResponse> {
        let tenant_id = Self::require_tenant(context)?;
        self.repository
            .retrieve_artifact(tenant_id, artifact_id)
            .await
    }

    async fn retain_artifact(
        &self,
        context: &DeployAppRequestContext,
        artifact_id: &str,
    ) -> DeployServiceResult<()> {
        let tenant_id = Self::require_tenant(context)?;
        self.repository
            .retain_artifact(tenant_id, artifact_id)
            .await
    }

    async fn list_env_variables(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        environment: Option<&str>,
    ) -> DeployServiceResult<sdkwork_deploy_contract::EnvVariablePage> {
        let tenant_id = Self::require_tenant(context)?;
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
        self.repository
            .create_env_variable(tenant_id, app_id, request)
            .await
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

    async fn list_health_checks(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
    ) -> DeployServiceResult<sdkwork_deploy_contract::HealthCheckPage> {
        let tenant_id = Self::require_tenant(context)?;
        self.repository.list_health_checks(tenant_id, app_id).await
    }

    async fn create_health_check(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        request: &CreateHealthCheckRequest,
    ) -> DeployServiceResult<sdkwork_deploy_contract::HealthCheckResponse> {
        let tenant_id = Self::require_tenant(context)?;
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
        if let Some(existing) = self
            .repository
            .find_upload_session_by_idempotency_key(tenant_id, &request.idempotency_key)
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
                    tenant_id,
                    request: request.clone(),
                },
            )
            .await?;
        match self
            .repository
            .create_upload_session_ref(tenant_id, context, request, &drive_response)
            .await
        {
            Ok(response) => Ok(response),
            Err(error @ sdkwork_deploy_contract::DeployServiceError::Conflict(_)) => {
                if let Some(existing) = self
                    .repository
                    .find_upload_session_by_idempotency_key(tenant_id, &request.idempotency_key)
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
        let stored = self
            .repository
            .retrieve_upload_session_ref(tenant_id, upload_session_id)
            .await?;
        let refreshed = self
            .drive
            .retrieve_upload_session(
                &Self::drive_credentials(context),
                &stored.drive_upload_session_id,
            )
            .await?;
        self.repository
            .update_upload_session_status(
                tenant_id,
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
        let stored = self
            .repository
            .retrieve_upload_session_ref(tenant_id, upload_session_id)
            .await?;
        Self::ensure_upload_session_mutable(&stored)?;
        let drive_response = self
            .drive
            .complete_upload_session(
                &Self::drive_credentials(context),
                &stored.drive_upload_session_id,
                request,
            )
            .await?;
        let updated = self
            .repository
            .update_upload_session_status(
                tenant_id,
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
        let stored = self
            .repository
            .retrieve_upload_session_ref(tenant_id, upload_session_id)
            .await?;
        Self::ensure_upload_session_mutable(&stored)?;
        let drive_response = self
            .drive
            .cancel_upload_session(
                &Self::drive_credentials(context),
                &stored.drive_upload_session_id,
            )
            .await?;
        self.repository
            .update_upload_session_status(
                tenant_id,
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
