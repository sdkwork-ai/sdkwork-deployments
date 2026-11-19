//! Repository port consumed by the deploy service layer.

use async_trait::async_trait;
use sdkwork_deploy_certificate_material::SealedCertificateFile;
use sdkwork_deploy_contract::DeployServiceResult;
use sdkwork_deploy_contract::{
    AcmeAccountPage, AcmeAccountResponse, AppDatabaseMigrationPage, AppDatabaseMigrationResponse,
    AppDatabaseProfilePage, AppDatabaseProfileResponse, AppDeploymentPage, AppDeploymentResponse,
    AppDomainPage, AppEnvironmentPage, AppEnvironmentResponse, AppPage, AppReleasePage,
    AppReleaseResponse, AppResponse, AppSourceKind, AppSourceResponse, AppTemplatePage,
    AppTemplateResponse, AppTemplateSummaryPage, AppTemplateVersionPage,
    AppTemplateVersionResponse, ArtifactPage, ArtifactResponse, AuditLogPage, BuildPage,
    BuildQueuePage, BuildResponse, BuildTemplatePage, BuildTemplateResponse,
    CertificateChallengePage, CertificateOrderPage, CertificateOrderResponse, CertificatePage,
    CertificateRenewalPage, CertificateResponse, ChannelPage, ChannelResponse, ChannelRolloutPage,
    ChannelRolloutResponse, CreateAcmeAccountRequest, CreateAppDatabaseMigrationRequest,
    CreateAppDatabaseProfileRequest, CreateAppDeploymentRequest, CreateAppEnvironmentRequest,
    CreateAppReleaseRequest, CreateAppRequest, CreateAppTemplateRequest,
    CreateAppTemplateVersionRequest, CreateArtifactRequest, CreateBuildRequest,
    CreateBuildTemplateRequest, CreateCertificateRequest, CreateDeployUploadSessionRequest,
    CreateDomainHostnameRequest, CreateDomainZoneRequest, CreateEnvVariableRequest,
    CreateHealthCheckRequest, CreateNginxConfigRequest, CreateNodeClusterRequest,
    CreatePlatformTargetRequest, CreateServerRequest, CreateSigningIdentityRequest,
    CreateSourceRepositoryRequest, CreateTemplateCategoryRequest, DeployUploadSessionResponse,
    DeploymentStatus, DomainDnsRecordPage, DomainDnsRecordResponse, DomainHostnamePage,
    DomainHostnameResponse, DomainZonePage, DomainZoneResponse, EntitlementProjectionPage,
    EnvVariablePage, EnvVariableResponse, EnvironmentPromotionPage, EnvironmentPromotionResponse,
    HealthCheckPage, HealthCheckResponse, ListAppTemplatesAdminQuery, ListAppTemplatesQuery,
    ListAppsQuery, ListDomainZonesQuery, ListMarketplaceTemplatesQuery, ListNginxConfigsQuery,
    NginxConfigPage, NginxConfigResponse, NginxReloadResponse, NginxStatusResponse,
    NginxValidateResponse, NodeClusterPage, NodeClusterResponse, PackagePage, PackageResponse,
    PlatformTargetPage, PlatformTargetResponse, PromoteChannelRequest, PromoteEnvironmentRequest,
    ProvisionAppDomainsResult, RegisterPackageRequest, ReleaseStatus,
    RequestCertificateOrderRequest, ResolvedDeployServer, RetentionRunResponse, RunnerHealthPage,
    ServerPage, ServerResponse, SigningIdentityHealthPage, SigningIdentityPage,
    SigningIdentityResponse, SourceEventPage, SourceEventResponse, SourceRepositoryPage,
    SourceRepositoryResponse, TemplateCategoryPage, TemplateCategoryResponse,
    UpdateAppDatabaseProfileRequest, UpdateAppEnvironmentRequest, UpdateAppRequest,
    UpdateAppSourceSpecRequest, UpdateAppTemplateAdminRequest, UpdateAppTemplateRequest,
    UpdateBuildStateRequest, UpdateDomainZoneRequest, UpdateNginxConfigRequest,
    UpdateNodeClusterRequest, UpdateServerRequest, UpdateTemplateCategoryRequest, UsageEventPage,
    UsageEventResponse, UsageReconciliationResponse,
};
use sdkwork_deploy_core::{GitRepositoryValidationStatus, GitValidationErrorCode};

use crate::{CertificateOrderCaaSubject, DomainVerificationChallenge};
use sdkwork_deploy_contract::{
    DownloadCardDto, DownloadCatalogDto, DownloadCatalogSummaryDto, DownloadItemDto,
    DownloadUpdateDto, UpsertDownloadCardRequest, UpsertDownloadCatalogRequest,
    UpsertDownloadItemRequest, UpsertDownloadUpdateRequest,
};
use sdkwork_deploy_contract::{UsageEventIngestItem, UsageEventQuery, UsageIngestResult};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InsertAuditLogCommand {
    pub tenant_id: i64,
    pub organization_id: i64,
    pub operator_id: i64,
    pub action: String,
    pub target_type: String,
    pub target_id: Option<i64>,
    pub target_uuid: Option<String>,
}

/// The deploy-application facts one IAM provisioning call needs, read back from
/// the committed `deploy_app` row.
///
/// `deploy_application_id` is the row's BIGINT `id` (never the uuid): IAM stores
/// it in `iam_tenant_application.deploy_application_id` under a unique index, and
/// the deployment access token's subject is that same value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppIdentityProvisioningTarget {
    /// `deploy_app.id` as a decimal string.
    pub deploy_application_id: String,
    pub tenant_id: String,
    /// `"0"` when the application carries no organization, matching IAM's own
    /// "no organization" convention.
    pub organization_id: String,
    /// Stable IAM `instance_key`: the application uuid, so a slug rename cannot
    /// look like a new installation.
    pub instance_key: String,
    pub display_name: String,
    pub environment: String,
    pub app_kind: String,
    /// `deploy_app.owner_type`: the ownership level the installation must
    /// mirror (`TENANT`, `ORGANIZATION`, `USER`, or the platform-internal
    /// `PLATFORM`).
    pub ownership_scope: String,
    /// `deploy_app.user_id`: the owner for a `USER` application, absent
    /// otherwise.
    pub owner_user_id: Option<String>,
    /// `deploy_app.identity_provisioning_key`, claimed (and persisted) by
    /// [`DeployRepositoryPort::begin_app_identity_provisioning`].
    pub provisioning_idempotency_key: String,
    /// `deploy_app.identity_status` before this attempt.
    pub identity_status: String,
}

/// Usage fact emitted by the service layer. The `deduplication_key` scoped to
/// the tenant makes delivery idempotent (build number, package checksum,
/// deployment uuid based) so retried flows never double-bill.
/// A source repository matched to a webhook payload.
#[derive(Clone, Debug)]
pub struct RepositoryMatch {
    pub tenant_id: i64,
    pub app_id: String,
    pub repository_id: String,
    pub repository_internal_id: i64,
    pub app_internal_id: i64,
    pub default_branch: String,
}

/// A build trigger candidate for a source event (active target with a
/// governed template).
#[derive(Clone, Debug)]
pub struct TriggerTarget {
    pub platform_target_id: String,
    pub template_id: String,
}

#[derive(Clone, Debug)]
pub struct InsertUsageEventCommand {
    pub tenant_id: i64,
    pub organization_id: i64,
    pub app_id: Option<i64>,
    /// Binding public internal id (`deploy_app_binding.id`) for per-domain
    /// traffic attribution.
    pub binding_id: Option<i64>,
    pub period_start: String,
    pub dimension: String,
    pub quantity: i64,
    pub unit: String,
    pub source_target_uuid: Option<String>,
    pub source_window_id: Option<String>,
    pub deduplication_key: String,
    /// Traffic attribution (hostname, server IP, app id, status class, …).
    pub attribution: Option<serde_json::Value>,
}

/// A certificate the renewal sweep has claimed, with everything needed to open
/// its order and to record what was handed over.
///
/// `previous_*` are captured at claim time on purpose: once the replacement is
/// stored the old version is superseded, and reading its window then would
/// describe the wrong certificate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CertificateRenewalClaim {
    pub tenant_id: i64,
    /// The renewal attempt's uuid, and the idempotency key's basis: retrying the
    /// same attempt must not open a second order.
    pub renewal_uuid: String,
    pub certificate_uuid: String,
    pub attempt_no: i32,
    pub renew_before_days: i32,
    pub previous_version_uuid: Option<String>,
    pub previous_not_before: Option<String>,
    pub previous_not_after: Option<String>,
}

/// A certificate order the issuance worker has claimed.
///
/// Carries the certificate's public uuid rather than its internal id, so the
/// worker reloads everything else through the same surface an HTTP request uses
/// and no part of the executor depends on physical row identity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CertificateOrderClaim {
    pub tenant_id: i64,
    pub order_uuid: String,
    pub certificate_uuid: String,
    /// The order status recorded when the claim was granted. This is the state
    /// machine's `from` value, so a claimed order never has to guess where it
    /// left off — and a reclaimed one resumes from wherever the previous worker
    /// actually stopped rather than from where it was expected to stop.
    pub status: String,
    /// Attempts consumed so far, including the one this claim just charged.
    pub attempt_count: i32,
    pub requested_version_no: i64,
}

/// What one expiry sweep changed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ExpiredCertificateSweep {
    /// Certificates moved from `ACTIVE` to `EXPIRED`.
    pub certificates_expired: i64,
    /// Versions moved from `ACTIVE` to `EXPIRED`.
    pub versions_expired: i64,
    /// Open renewal attempts cancelled because the certificate they were
    /// replacing expired first.
    pub renewals_cancelled: i64,
}

impl ExpiredCertificateSweep {
    /// Whether the sweep changed nothing.
    ///
    /// A sweep runs on a timer for as long as the service is up, so the caller
    /// needs to distinguish "housekeeping ran" from "something retired" before
    /// deciding whether it is worth a log line.
    pub fn is_empty(&self) -> bool {
        self.certificates_expired == 0 && self.versions_expired == 0 && self.renewals_cancelled == 0
    }
}

/// Where a hostname's `_acme-challenge` record has to be published, and which
/// cloud account has authority to publish it.
///
/// Resolved from the hostname because that is what an issuance order carries: the
/// order knows the names it must prove, and the zone that owns them is a lookup
/// away. Looking it up here rather than threading it through the order keeps the
/// order's own rows independent of the domain inventory, which is what lets a
/// certificate be re-issued after its zone was re-pinned.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DnsChallengeZone {
    /// The zone apex that owns the record. `_acme-challenge` is published
    /// relative to this, not to the requested hostname.
    pub zone_apex: String,
    /// The DNS family the zone declared when it was created, in the canonical
    /// uppercase spelling. `None` when the zone never recorded one, in which case
    /// the account's own vendor is the only statement of who serves the records.
    pub dns_provider: Option<String>,
    /// The provider-side zone identity, for families that address a zone by an
    /// opaque id instead of by name.
    pub provider_zone_ref: Option<String>,
    /// The cloud account pinned to this zone, if any.
    pub provider_account_id: Option<String>,
}

/// The Zone a resolution-records sync targets.
///
/// The snapshot rows key on the internal id, the provider inventory is read
/// for the apex, and the account binding decides which credential answers —
/// the same three facts the sync operation needs, carried once so the store
/// read and the provider call cannot disagree about which Zone they describe.
#[derive(Clone, Debug)]
pub struct ZoneDnsSyncTarget {
    pub zone_id: i64,
    pub apex_hostname: String,
    pub dns_provider: Option<String>,
    pub provider_account_id: Option<String>,
}

/// One active hostname of a Zone, for the sync's record-to-hostname match.
#[derive(Clone, Debug)]
pub struct DomainHostnameAsset {
    pub domain_id: i64,
    pub hostname_ascii: String,
    pub hostname_type: String,
}

/// One row of a provider inventory read, expressed in store terms.
///
/// The service converts the provider adapter's record type into this row so
/// the repository depends on no DNS adapter: the store writes what it is
/// given, and the vocabulary of providers stops at the service boundary.
#[derive(Clone, Debug)]
pub struct DnsRecordSnapshotRow {
    pub record_name: String,
    pub record_type: String,
    pub record_value: String,
    pub ttl_seconds: Option<i32>,
    pub priority: Option<i32>,
    pub record_line: Option<String>,
    pub provider_record_ref: Option<String>,
    /// The hostname this record resolves, matched by the service before the
    /// write; `None` for a record whose owner matches no registered hostname.
    pub domain_id: Option<i64>,
}

/// One sync run's whole write: the read's metadata plus every row.
///
/// The store replaces the Zone's snapshot in one transaction, so this carries
/// the complete inventory rather than a diff — the truth is "what the provider
/// answered now", not a merge with a previous answer.
#[derive(Clone, Debug)]
pub struct DomainDnsSnapshotWrite {
    pub dns_provider: String,
    pub provider_account_id: String,
    pub synced_at: String,
    pub records: Vec<DnsRecordSnapshotRow>,
}

/// The restrictions of a snapshot read.
///
/// `hostname_id` selects one hostname's rows — the sync already applied the
/// wildcard semantics when it stamped `domain_id`. `host` is the 主机记录
/// keyword filter (substring, case-insensitive) and `record_type` the exact
/// type filter — the pair Aliyun's own 解析设置 page offers. Absent members
/// filter nothing.
#[derive(Clone, Debug, Default)]
pub struct DomainDnsRecordFilter {
    pub hostname_id: Option<String>,
    pub host: Option<String>,
    pub record_type: Option<String>,
}

/// One resolution-record write, expressed in store terms: the snapshot row
/// fields a create/update produces, with the hostname match already applied.
#[derive(Clone, Debug)]
pub struct DomainDnsRecordUpsert {
    pub record_name: String,
    pub record_type: String,
    pub record_value: String,
    pub ttl_seconds: Option<i32>,
    pub priority: Option<i32>,
    pub record_line: Option<String>,
    pub domain_id: Option<i64>,
    pub dns_provider: String,
    pub provider_account_id: String,
    pub provider_record_ref: Option<String>,
}

/// Ownership and marketplace state of one listing, for the install-flow
/// authorization decision.
///
/// Deliberately metadata only: a caller that may not read the listing still
/// receives no listing body, and a draft's existence stays indistinguishable
/// from a missing row because the service answers both as "not found".
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TemplateReadScope {
    pub author_user_id: i64,
    pub visibility: String,
    pub status: String,
}

// The trait's methods are grouped by the aggregate they read or write.
//
// `AppSourceSpecRepositoryPort` is a supertrait rather than another block of
// methods here because the source-spec aggregate has its own port shape: it is
// implemented by the same repository but its CRUD surface is derived from the
// spec contract, not from the deploy repository's request-level methods.
#[async_trait]
pub trait DeployRepositoryPort:
    crate::AppCompositionRepositoryPort
    + crate::AppResourceLinkRepositoryPort
    + crate::AppSourceSpecRepositoryPort
    + Send
    + Sync
{
    async fn ready_check(&self) -> DeployServiceResult<()>;

    /// Resolves the zone that owns `hostname` and the account pinned to it.
    ///
    /// Answers `Ok(None)` for a hostname this tenant has no zone for, which is a
    /// normal state rather than an error: a certificate may cover a name whose zone
    /// rows were never created here, and an issuance then falls back to the
    /// deployment-level provider configuration.
    async fn retrieve_dns_challenge_zone(
        &self,
        tenant_id: i64,
        hostname: &str,
    ) -> DeployServiceResult<Option<DnsChallengeZone>>;

    /// Lists the root domains this caller owns, plus the tenant-level zones
    /// that carry no owner.
    ///
    /// `owner_user_id` is the caller's own subject. The domain inventory is
    /// user-private (`IAM_SPEC.md` 5.1: a personal session's data domain is the
    /// authenticated user's own data), so a zone owned by somebody else is not
    /// listed even though it lives in the same tenant. NULL-owner zones are the
    /// platform's `app.<suffix>` inventory, which is tenant-level by nature and
    /// stays visible to every member.
    async fn list_domain_zones(
        &self,
        tenant_id: i64,
        owner_user_id: Option<i64>,
        query: &ListDomainZonesQuery,
    ) -> DeployServiceResult<DomainZonePage>;

    async fn create_domain_zone(
        &self,
        tenant_id: i64,
        organization_id: Option<i64>,
        actor_id: Option<i64>,
        request: &CreateDomainZoneRequest,
    ) -> DeployServiceResult<DomainZoneResponse>;

    /// `owner_user_id` scopes the read to the caller's own zones; see
    /// [`Self::list_domain_zones`].
    async fn retrieve_domain_zone(
        &self,
        tenant_id: i64,
        owner_user_id: Option<i64>,
        zone_id: &str,
    ) -> DeployServiceResult<DomainZoneResponse>;

    async fn update_domain_zone(
        &self,
        tenant_id: i64,
        actor_id: Option<i64>,
        zone_id: &str,
        request: &UpdateDomainZoneRequest,
    ) -> DeployServiceResult<DomainZoneResponse>;

    /// `owner_user_id` scopes the read to the caller's own zones; see
    /// [`Self::list_domain_zones`].
    async fn delete_domain_zone(
        &self,
        tenant_id: i64,
        owner_user_id: Option<i64>,
        zone_id: &str,
    ) -> DeployServiceResult<()>;

    /// Every verification file registered on the Zone, platform order first.
    async fn list_dns_zone_verification_files(
        &self,
        tenant_id: i64,
        owner_user_id: Option<i64>,
        zone_id: &str,
    ) -> DeployServiceResult<Vec<crate::verification_file::VerificationFileRecord>>;

    /// Registers a verification file, replacing the row with the same
    /// `(zone, file_name)` — a platform that reissues its file replaces the
    /// previous one whole.
    async fn upsert_dns_zone_verification_file(
        &self,
        tenant_id: i64,
        owner_user_id: Option<i64>,
        zone_id: &str,
        platform: &str,
        file_name: &str,
        content: &str,
    ) -> DeployServiceResult<crate::verification_file::VerificationFileRecord>;

    /// Reads one verification file by its own id; `not_found` when the zone has
    /// no such file.
    async fn retrieve_dns_zone_verification_file(
        &self,
        tenant_id: i64,
        owner_user_id: Option<i64>,
        zone_id: &str,
        file_id: &str,
    ) -> DeployServiceResult<crate::verification_file::VerificationFileRecord>;

    /// Deletes one verification file; `false` when the zone had no such file.
    async fn delete_dns_zone_verification_file(
        &self,
        tenant_id: i64,
        owner_user_id: Option<i64>,
        zone_id: &str,
        file_id: &str,
    ) -> DeployServiceResult<bool>;

    /// `owner_user_id` scopes the read to the caller's own zones; see
    /// [`Self::list_domain_zones`].
    async fn list_domain_hostnames(
        &self,
        tenant_id: i64,
        owner_user_id: Option<i64>,
        zone_id: &str,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<DomainHostnamePage>;

    /// The Zone a resolution-records sync targets, or `None` when the id is
    /// unknown (the sync refuses rather than guessing an apex).
    async fn domain_zone_dns_sync_target(
        &self,
        tenant_id: i64,
        owner_user_id: Option<i64>,
        zone_id: &str,
    ) -> DeployServiceResult<Option<ZoneDnsSyncTarget>>;

    /// Every active hostname of the Zone, for the sync's record match.
    /// Bounded by the Zone's hostname inventory, which is why it is a plain
    /// read rather than a paged one.
    async fn list_domain_hostname_assets(
        &self,
        tenant_id: i64,
        owner_user_id: Option<i64>,
        zone_id: &str,
    ) -> DeployServiceResult<Vec<DomainHostnameAsset>>;

    /// Replaces the Zone's whole resolution-record snapshot in one
    /// transaction and returns the stored row count. The Zone row is locked
    /// for the write, so a concurrent delete cannot free the zone under the
    /// snapshot.
    async fn replace_domain_zone_dns_records(
        &self,
        tenant_id: i64,
        owner_user_id: Option<i64>,
        zone_id: &str,
        snapshot: &DomainDnsSnapshotWrite,
    ) -> DeployServiceResult<i64>;

    /// Pages the stored snapshot, optionally restricted to one hostname.
    /// Store-level pagination: the page bounds bound the query, not a
    /// post-read slice. `owner_user_id` scopes the read to the caller's own
    /// zones; see [`Self::list_domain_zones`].
    async fn list_domain_zone_dns_records(
        &self,
        tenant_id: i64,
        owner_user_id: Option<i64>,
        zone_id: &str,
        filter: &DomainDnsRecordFilter,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<DomainDnsRecordPage>;

    /// Appends one write-through row to the stored snapshot, answered with the
    /// stored row's uuid. The provider write has already happened; this is the
    /// join so the page reads one table.
    async fn insert_domain_zone_dns_record(
        &self,
        tenant_id: i64,
        owner_user_id: Option<i64>,
        zone_id: &str,
        record: &DomainDnsRecordUpsert,
    ) -> DeployServiceResult<DomainDnsRecordResponse>;

    /// Replaces one stored row in place after a provider update.
    async fn update_domain_zone_dns_record(
        &self,
        tenant_id: i64,
        owner_user_id: Option<i64>,
        zone_id: &str,
        record_id: &str,
        record: &DomainDnsRecordUpsert,
    ) -> DeployServiceResult<DomainDnsRecordResponse>;

    /// Flips one stored row's provider-side state after the vendor accepted
    /// the pause/resume.
    async fn set_domain_zone_dns_record_status(
        &self,
        tenant_id: i64,
        owner_user_id: Option<i64>,
        zone_id: &str,
        record_id: &str,
        enabled: bool,
    ) -> DeployServiceResult<DomainDnsRecordResponse>;

    /// Removes one stored row after the provider confirmed the delete.
    async fn delete_domain_zone_dns_record(
        &self,
        tenant_id: i64,
        owner_user_id: Option<i64>,
        zone_id: &str,
        record_id: &str,
    ) -> DeployServiceResult<()>;

    /// Loads one stored row's provider record ref, for the write that has to
    /// address the vendor by its own id.
    async fn domain_zone_dns_record_ref(
        &self,
        tenant_id: i64,
        owner_user_id: Option<i64>,
        zone_id: &str,
        record_id: &str,
    ) -> DeployServiceResult<Option<String>>;

    async fn create_domain_hostname(
        &self,
        tenant_id: i64,
        actor_id: Option<i64>,
        zone_id: &str,
        request: &CreateDomainHostnameRequest,
    ) -> DeployServiceResult<DomainHostnameResponse>;

    /// Declare-or-reuse a hostname by relative name; see
    /// [`Self::create_domain_hostname`] for the strict, operator-driven variant.
    async fn ensure_domain_hostname(
        &self,
        tenant_id: i64,
        actor_id: Option<i64>,
        zone_id: &str,
        relative_name: &str,
    ) -> DeployServiceResult<DomainHostnameResponse>;

    /// `owner_user_id` scopes the read to the caller's own zones; see
    /// [`Self::list_domain_zones`].
    async fn retrieve_domain_hostname(
        &self,
        tenant_id: i64,
        owner_user_id: Option<i64>,
        zone_id: &str,
        hostname_id: &str,
    ) -> DeployServiceResult<DomainHostnameResponse>;

    async fn update_domain_hostname(
        &self,
        tenant_id: i64,
        actor_id: Option<i64>,
        zone_id: &str,
        hostname_id: &str,
        request: &sdkwork_deploy_contract::UpdateDomainHostnameRequest,
    ) -> DeployServiceResult<DomainHostnameResponse>;

    /// `owner_user_id` scopes the read to the caller's own zones; see
    /// [`Self::list_domain_zones`].
    async fn delete_domain_hostname(
        &self,
        tenant_id: i64,
        owner_user_id: Option<i64>,
        zone_id: &str,
        hostname_id: &str,
    ) -> DeployServiceResult<()>;

    /// `owner_user_id` scopes the read to the caller's own zones; see
    /// [`Self::list_domain_zones`].
    async fn domain_hostname_verification_challenge(
        &self,
        tenant_id: i64,
        owner_user_id: Option<i64>,
        zone_id: &str,
        hostname_id: &str,
    ) -> DeployServiceResult<DomainVerificationChallenge>;

    /// `owner_user_id` scopes the read to the caller's own zones; see
    /// [`Self::list_domain_zones`].
    async fn confirm_domain_hostname_verification(
        &self,
        tenant_id: i64,
        owner_user_id: Option<i64>,
        zone_id: &str,
        hostname_id: &str,
        verification_id: &str,
        observed_sha256: &str,
        verifier_identity: &str,
    ) -> DeployServiceResult<bool>;

    async fn set_app_status(
        &self,
        tenant_id: i64,
        app_id: &str,
        status: i32,
    ) -> DeployServiceResult<AppResponse>;

    /// Lists artifacts visible to the caller.
    ///
    /// `app_scope` is the deploy application the caller's credential proves
    /// (`APPLICATION_IDENTITY_SPEC.md` §4.3 — data-plane queries filter by
    /// `(tenant_id, app_id)`). `None` keeps the tenant-wide listing for a caller
    /// whose credential names no application.
    async fn list_artifacts(
        &self,
        tenant_id: i64,
        app_scope: Option<&str>,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<ArtifactPage>;

    /// Registers an artifact, binding its `app_id` to `app_scope` when present.
    async fn create_artifact_from_drive(
        &self,
        tenant_id: i64,
        app_scope: Option<&str>,
        request: &CreateArtifactRequest,
    ) -> DeployServiceResult<ArtifactResponse>;

    async fn retrieve_artifact(
        &self,
        tenant_id: i64,
        app_scope: Option<&str>,
        artifact_id: &str,
    ) -> DeployServiceResult<ArtifactResponse>;

    async fn retain_artifact(
        &self,
        tenant_id: i64,
        app_scope: Option<&str>,
        artifact_id: &str,
    ) -> DeployServiceResult<()>;

    async fn create_artifact_from_upload_session(
        &self,
        tenant_id: i64,
        app_scope: Option<&str>,
        upload_session_id: &str,
        checksum_sha256: &str,
    ) -> DeployServiceResult<ArtifactResponse>;

    async fn list_env_variables(
        &self,
        tenant_id: i64,
        app_id: &str,
        environment: Option<&str>,
    ) -> DeployServiceResult<EnvVariablePage>;

    async fn create_env_variable(
        &self,
        tenant_id: i64,
        app_id: &str,
        request: &CreateEnvVariableRequest,
    ) -> DeployServiceResult<EnvVariableResponse>;

    async fn list_certificates(
        &self,
        tenant_id: i64,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<CertificatePage>;

    async fn create_certificate(
        &self,
        tenant_id: i64,
        organization_id: Option<i64>,
        actor_id: Option<i64>,
        idempotency_key: &str,
        request: &CreateCertificateRequest,
    ) -> DeployServiceResult<CertificateResponse>;

    async fn retrieve_certificate(
        &self,
        tenant_id: i64,
        certificate_id: &str,
    ) -> DeployServiceResult<CertificateResponse>;

    async fn delete_certificate(
        &self,
        tenant_id: i64,
        certificate_id: &str,
    ) -> DeployServiceResult<()>;

    async fn renew_certificate(
        &self,
        tenant_id: i64,
        certificate_id: &str,
    ) -> DeployServiceResult<CertificateResponse>;

    async fn list_health_checks(
        &self,
        tenant_id: i64,
        app_id: &str,
    ) -> DeployServiceResult<HealthCheckPage>;

    async fn create_health_check(
        &self,
        tenant_id: i64,
        app_id: &str,
        request: &CreateHealthCheckRequest,
    ) -> DeployServiceResult<HealthCheckResponse>;

    async fn list_nginx_configs(
        &self,
        tenant_id: Option<i64>,
        query: &ListNginxConfigsQuery,
    ) -> DeployServiceResult<NginxConfigPage>;

    async fn create_nginx_config(
        &self,
        tenant_id: i64,
        request: &CreateNginxConfigRequest,
    ) -> DeployServiceResult<NginxConfigResponse>;

    async fn retrieve_nginx_config(
        &self,
        tenant_id: Option<i64>,
        config_id: &str,
    ) -> DeployServiceResult<NginxConfigResponse>;

    async fn update_nginx_config(
        &self,
        tenant_id: Option<i64>,
        config_id: &str,
        request: &UpdateNginxConfigRequest,
    ) -> DeployServiceResult<NginxConfigResponse>;

    async fn validate_nginx_config(
        &self,
        tenant_id: Option<i64>,
        config_id: &str,
    ) -> DeployServiceResult<NginxValidateResponse>;

    async fn deploy_nginx_config(
        &self,
        tenant_id: Option<i64>,
        config_id: &str,
    ) -> DeployServiceResult<NginxConfigResponse>;

    async fn reload_nginx(&self) -> DeployServiceResult<NginxReloadResponse>;

    async fn retrieve_nginx_status(
        &self,
        tenant_id: Option<i64>,
    ) -> DeployServiceResult<NginxStatusResponse>;

    async fn list_servers(
        &self,
        tenant_id: i64,
        page: i32,
        page_size: i32,
        cluster_id: Option<String>,
    ) -> DeployServiceResult<ServerPage>;

    async fn create_server(
        &self,
        tenant_id: i64,
        request: &CreateServerRequest,
    ) -> DeployServiceResult<ServerResponse>;

    async fn update_server(
        &self,
        tenant_id: i64,
        server_id: &str,
        request: &UpdateServerRequest,
    ) -> DeployServiceResult<ServerResponse>;

    async fn list_node_clusters(
        &self,
        tenant_id: i64,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<NodeClusterPage>;

    async fn create_node_cluster(
        &self,
        tenant_id: i64,
        request: &CreateNodeClusterRequest,
    ) -> DeployServiceResult<NodeClusterResponse>;

    async fn update_node_cluster(
        &self,
        tenant_id: i64,
        cluster_id: &str,
        request: &UpdateNodeClusterRequest,
    ) -> DeployServiceResult<NodeClusterResponse>;

    async fn list_audit_logs(
        &self,
        tenant_id: Option<i64>,
        query: &sdkwork_deploy_contract::AuditLogQuery,
        cursor: Option<&str>,
    ) -> DeployServiceResult<AuditLogPage>;

    async fn insert_audit_log(&self, command: InsertAuditLogCommand) -> DeployServiceResult<()>;

    /// Records the Drive-backed upload session, binding its `app_id` to
    /// `app_scope` when the caller's credential proves one.
    async fn create_upload_session_ref(
        &self,
        tenant_id: i64,
        app_scope: Option<&str>,
        request: &CreateDeployUploadSessionRequest,
        drive: &DeployUploadSessionResponse,
    ) -> DeployServiceResult<DeployUploadSessionResponse>;

    /// Idempotency replay lookup, narrowed to the caller's application when the
    /// credential proves one: two applications in a tenant must not observe each
    /// other's sessions through a shared key.
    async fn find_upload_session_by_idempotency_key(
        &self,
        tenant_id: i64,
        app_scope: Option<&str>,
        idempotency_key: &str,
    ) -> DeployServiceResult<Option<DeployUploadSessionResponse>>;

    async fn retrieve_upload_session_ref(
        &self,
        tenant_id: i64,
        app_scope: Option<&str>,
        upload_session_id: &str,
    ) -> DeployServiceResult<DeployUploadSessionResponse>;

    async fn update_upload_session_status(
        &self,
        tenant_id: i64,
        app_scope: Option<&str>,
        upload_session_id: &str,
        status: i32,
        drive_node_id: Option<&str>,
    ) -> DeployServiceResult<DeployUploadSessionResponse>;

    // -- unified app delivery (REQ-2026-0002) --------------------------------

    async fn create_app(
        &self,
        tenant_id: i64,
        organization_id: Option<i64>,
        actor_id: Option<i64>,
        idempotency_key: Option<&str>,
        request: &CreateAppRequest,
    ) -> DeployServiceResult<AppResponse>;

    /// List the apps this subject may reach.
    ///
    /// `actor_id` and `organization_id` are the caller's subject, not filters:
    /// they drive the reachability gate inside the implementation, so a tenant
    /// member sees the shared levels plus their own apps rather than the whole
    /// tenant. `query.ownership_scope` / `query.organization_id` / `query.app_kind`
    /// only narrow further.
    async fn list_apps(
        &self,
        tenant_id: i64,
        actor_id: Option<i64>,
        organization_id: Option<i64>,
        query: &ListAppsQuery,
    ) -> DeployServiceResult<AppPage>;

    /// The operator ledger: no owner gate, optional tenant scope.
    ///
    /// `tenant_filter` is `None` only for an operator context that carries no
    /// tenant; every other caller resolves to exactly one tenant.
    async fn list_apps_admin(
        &self,
        tenant_filter: Option<i64>,
        query: &ListAppsQuery,
    ) -> DeployServiceResult<AppPage>;

    async fn retrieve_app(&self, tenant_id: i64, app_id: &str) -> DeployServiceResult<AppResponse>;

    async fn update_app(
        &self,
        tenant_id: i64,
        actor_id: Option<i64>,
        organization_id: Option<i64>,
        app_id: &str,
        request: &UpdateAppRequest,
    ) -> DeployServiceResult<AppResponse>;

    // -- IAM identity provisioning (migration 0008) ---------------------------

    /// Reads `deploy_app.identity_status` without claiming the provisioning key.
    ///
    /// [`Self::begin_app_identity_provisioning`] claims the key as a side effect.
    /// A retry entry point must decide "already active → do nothing" *before*
    /// that, otherwise a no-op call would write a key onto an `active` row that
    /// has none — the exact anomaly the reconciliation runbook says to report
    /// rather than silently normalise. This read is therefore separate and
    /// side-effect free.
    async fn retrieve_app_identity_status(
        &self,
        tenant_id: i64,
        app_id: &str,
    ) -> DeployServiceResult<String>;

    /// Claims the application's stable IAM provisioning key and reads back the
    /// fields a provisioning call needs.
    ///
    /// The key is `deploy:{deploy_app.id}`: derived from the row's own primary
    /// key, so it is stable across retries and across processes without a
    /// read-modify-write race, and `COALESCE` keeps whatever an earlier attempt
    /// already persisted. `uk_deploy_app_identity_provisioning_key` is what
    /// makes a second, different key for the same application impossible.
    async fn begin_app_identity_provisioning(
        &self,
        tenant_id: i64,
        app_id: &str,
    ) -> DeployServiceResult<AppIdentityProvisioningTarget>;

    /// Records the outcome of one IAM provisioning attempt on `deploy_app`.
    ///
    /// `status` must be one of `pending_identity` | `active` | `failed`; the
    /// column's CHECK constraint rejects anything else, which is the intended
    /// failure mode of a typo here.
    async fn record_app_identity_status(
        &self,
        tenant_id: i64,
        app_id: &str,
        status: &str,
    ) -> DeployServiceResult<()>;

    async fn list_app_domains(
        &self,
        tenant_id: i64,
        app_id: &str,
    ) -> DeployServiceResult<AppDomainPage>;

    async fn create_platform_target(
        &self,
        tenant_id: i64,
        app_id: &str,
        actor_id: Option<i64>,
        request: &CreatePlatformTargetRequest,
    ) -> DeployServiceResult<PlatformTargetResponse>;

    async fn list_platform_targets(
        &self,
        tenant_id: i64,
        app_id: &str,
    ) -> DeployServiceResult<PlatformTargetPage>;

    async fn retrieve_platform_target(
        &self,
        tenant_id: i64,
        app_id: &str,
        target_id: &str,
    ) -> DeployServiceResult<PlatformTargetResponse>;

    async fn create_source_repository(
        &self,
        tenant_id: i64,
        app_id: &str,
        actor_id: Option<i64>,
        request: &CreateSourceRepositoryRequest,
    ) -> DeployServiceResult<SourceRepositoryResponse>;

    async fn list_source_repositories(
        &self,
        tenant_id: i64,
        app_id: &str,
    ) -> DeployServiceResult<SourceRepositoryPage>;

    async fn retrieve_source_repository(
        &self,
        tenant_id: i64,
        app_id: &str,
        repo_id: &str,
    ) -> DeployServiceResult<SourceRepositoryResponse>;

    async fn create_build_template(
        &self,
        tenant_id: i64,
        actor_id: Option<i64>,
        request: &CreateBuildTemplateRequest,
    ) -> DeployServiceResult<BuildTemplateResponse>;

    async fn list_build_templates(
        &self,
        tenant_id: i64,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<BuildTemplatePage>;

    async fn retrieve_build_template(
        &self,
        tenant_id: i64,
        template_id: &str,
    ) -> DeployServiceResult<BuildTemplateResponse>;

    async fn create_build(
        &self,
        tenant_id: i64,
        actor_id: Option<i64>,
        request: &CreateBuildRequest,
    ) -> DeployServiceResult<BuildResponse>;

    async fn list_builds(
        &self,
        tenant_id: i64,
        app_id: &str,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<BuildPage>;

    async fn retrieve_build(
        &self,
        tenant_id: i64,
        app_id: &str,
        build_id: &str,
    ) -> DeployServiceResult<BuildResponse>;

    async fn update_build_state(
        &self,
        tenant_id: i64,
        app_id: &str,
        build_id: &str,
        request: &UpdateBuildStateRequest,
    ) -> DeployServiceResult<BuildResponse>;

    async fn claim_next_build(
        &self,
        tenant_id: i64,
        runner_node_uuid: &str,
        runner_version: &str,
    ) -> DeployServiceResult<Option<BuildResponse>>;

    /// Resolves (app uuid, platform target uuid, platform) for a build so
    /// package registration can enforce platform/format rules.
    async fn resolve_build_platform(
        &self,
        tenant_id: i64,
        build_id: &str,
    ) -> DeployServiceResult<(String, String, String)>;

    async fn register_package(
        &self,
        tenant_id: i64,
        actor_id: Option<i64>,
        request: &RegisterPackageRequest,
    ) -> DeployServiceResult<PackageResponse>;

    async fn list_packages(
        &self,
        tenant_id: i64,
        app_id: &str,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<PackagePage>;

    async fn retrieve_package(
        &self,
        tenant_id: i64,
        app_id: &str,
        package_id: &str,
    ) -> DeployServiceResult<PackageResponse>;

    async fn create_app_release(
        &self,
        tenant_id: i64,
        actor_id: Option<i64>,
        request: &CreateAppReleaseRequest,
    ) -> DeployServiceResult<AppReleaseResponse>;

    async fn list_app_releases(
        &self,
        tenant_id: i64,
        app_id: &str,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<AppReleasePage>;

    async fn retrieve_app_release(
        &self,
        tenant_id: i64,
        app_id: &str,
        release_id: &str,
    ) -> DeployServiceResult<AppReleaseResponse>;

    async fn update_app_release_status(
        &self,
        tenant_id: i64,
        app_id: &str,
        release_id: &str,
        release_status: ReleaseStatus,
    ) -> DeployServiceResult<AppReleaseResponse>;

    async fn ensure_release_channel(
        &self,
        tenant_id: i64,
        app_id: &str,
        target_id: &str,
        channel_key: &str,
    ) -> DeployServiceResult<ChannelResponse>;

    async fn retrieve_channel(
        &self,
        tenant_id: i64,
        app_id: &str,
        channel_id: &str,
    ) -> DeployServiceResult<ChannelResponse>;

    async fn list_channels(&self, tenant_id: i64, app_id: &str)
        -> DeployServiceResult<ChannelPage>;

    async fn promote_channel(
        &self,
        tenant_id: i64,
        app_id: &str,
        channel_id: &str,
        actor_id: Option<i64>,
        request: &PromoteChannelRequest,
    ) -> DeployServiceResult<ChannelRolloutResponse>;

    async fn list_channel_rollouts(
        &self,
        tenant_id: i64,
        app_id: &str,
        channel_id: &str,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<ChannelRolloutPage>;

    async fn create_app_deployment(
        &self,
        tenant_id: i64,
        actor_id: Option<i64>,
        request: &CreateAppDeploymentRequest,
    ) -> DeployServiceResult<AppDeploymentResponse>;

    async fn list_app_deployments(
        &self,
        tenant_id: i64,
        app_id: &str,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<AppDeploymentPage>;

    async fn retrieve_app_deployment(
        &self,
        tenant_id: i64,
        app_id: &str,
        deployment_id: &str,
    ) -> DeployServiceResult<AppDeploymentResponse>;

    /// Lists deployments currently in platform review states for
    /// review-observation polling.
    async fn list_review_pending_deployments(
        &self,
        tenant_id: i64,
        limit: i64,
    ) -> DeployServiceResult<Vec<AppDeploymentResponse>>;

    async fn update_app_deployment_state(
        &self,
        tenant_id: i64,
        app_id: &str,
        deployment_id: &str,
        deployment_status: DeploymentStatus,
        platform_review_ref: Option<&str>,
    ) -> DeployServiceResult<AppDeploymentResponse>;

    async fn create_signing_identity(
        &self,
        tenant_id: i64,
        actor_id: Option<i64>,
        request: &CreateSigningIdentityRequest,
    ) -> DeployServiceResult<SigningIdentityResponse>;

    async fn list_signing_identities(
        &self,
        tenant_id: i64,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<SigningIdentityPage>;

    async fn retrieve_signing_identity(
        &self,
        tenant_id: i64,
        identity_id: &str,
    ) -> DeployServiceResult<SigningIdentityResponse>;

    /// Records one usage fact (idempotent on the tenant deduplication key);
    /// metering failures must never block the primary operation.
    async fn insert_usage_event(
        &self,
        command: &InsertUsageEventCommand,
    ) -> DeployServiceResult<UsageEventResponse>;

    async fn list_usage_events(
        &self,
        tenant_id: i64,
        query: &UsageEventQuery,
    ) -> DeployServiceResult<UsageEventPage>;

    /// Batch-ingest traffic usage events submitted by a Web Server node.
    /// Each event is deduplicated; binding/site uuids are resolved to
    /// internal ids and tenant attribution is derived from the binding when
    /// the node could not attribute it.
    async fn insert_usage_events_batch(
        &self,
        events: &[UsageEventIngestItem],
    ) -> DeployServiceResult<UsageIngestResult>;

    async fn create_app_database_profile(
        &self,
        tenant_id: i64,
        actor_id: Option<i64>,
        app_id: &str,
        request: &CreateAppDatabaseProfileRequest,
    ) -> DeployServiceResult<AppDatabaseProfileResponse>;

    async fn list_app_database_profiles(
        &self,
        tenant_id: i64,
        app_id: &str,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<AppDatabaseProfilePage>;

    async fn retrieve_app_database_profile(
        &self,
        tenant_id: i64,
        app_id: &str,
        profile_id: &str,
    ) -> DeployServiceResult<AppDatabaseProfileResponse>;

    async fn update_app_database_profile(
        &self,
        tenant_id: i64,
        actor_id: Option<i64>,
        app_id: &str,
        profile_id: &str,
        request: &UpdateAppDatabaseProfileRequest,
    ) -> DeployServiceResult<AppDatabaseProfileResponse>;

    async fn create_app_database_migration(
        &self,
        tenant_id: i64,
        actor_id: Option<i64>,
        app_id: &str,
        profile_id: &str,
        request: &CreateAppDatabaseMigrationRequest,
    ) -> DeployServiceResult<AppDatabaseMigrationResponse>;

    async fn list_app_database_migrations(
        &self,
        tenant_id: i64,
        app_id: &str,
        profile_id: &str,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<AppDatabaseMigrationPage>;

    async fn retrieve_app_database_migration(
        &self,
        tenant_id: i64,
        app_id: &str,
        profile_id: &str,
        migration_id: &str,
    ) -> DeployServiceResult<AppDatabaseMigrationResponse>;

    /// Current tenant usage for one entitlement dimension (enforcement
    /// evidence; tenant-scoped aggregate).
    async fn entitlement_usage(&self, tenant_id: i64, dimension: &str) -> DeployServiceResult<i64>;

    async fn list_entitlement_projections(
        &self,
        tenant_id: Option<i64>,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<EntitlementProjectionPage>;

    async fn list_build_queue(
        &self,
        tenant_id: Option<i64>,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<BuildQueuePage>;

    async fn list_runner_health(
        &self,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<RunnerHealthPage>;

    async fn create_acme_account(
        &self,
        tenant_id: i64,
        request: &CreateAcmeAccountRequest,
    ) -> DeployServiceResult<AcmeAccountResponse>;

    async fn list_acme_accounts(
        &self,
        tenant_id: i64,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<AcmeAccountPage>;

    async fn request_certificate_order(
        &self,
        tenant_id: i64,
        request: &RequestCertificateOrderRequest,
    ) -> DeployServiceResult<CertificateOrderResponse>;

    /// The identifiers, resolved validation method, and ACME directory a
    /// certificate order would use.
    ///
    /// Feeds the CAA pre-flight, which must run before an order exists. The
    /// directory comes from the same account resolution the order itself uses,
    /// so the CAA check and the CA that will be asked are guaranteed to agree.
    async fn certificate_order_caa_subject(
        &self,
        tenant_id: i64,
        certificate_id: &str,
    ) -> DeployServiceResult<CertificateOrderCaaSubject>;

    /// Records the CAA pre-issuance decision and the instant it was observed on
    /// an order, so a refusal is auditable without re-querying DNS.
    async fn record_certificate_order_caa_decision(
        &self,
        tenant_id: i64,
        order_id: &str,
        decision: &str,
        checked_at: &str,
    ) -> DeployServiceResult<()>;

    async fn advance_certificate_order(
        &self,
        tenant_id: i64,
        order_id: &str,
        from_status: &str,
        to_status: &str,
    ) -> DeployServiceResult<String>;

    async fn fail_certificate_order(
        &self,
        tenant_id: i64,
        order_id: &str,
        error_code: &str,
    ) -> DeployServiceResult<()>;

    async fn record_challenge_result(
        &self,
        tenant_id: i64,
        order_id: &str,
        challenge_id: Option<&str>,
        valid: bool,
        error_code: Option<&str>,
    ) -> DeployServiceResult<()>;

    /// Stores an issued certificate version together with its sealed material.
    ///
    /// `certificate_version_uuid` is supplied by the caller rather than generated
    /// here, because the material was sealed under it: the AAD binds each file to
    /// that uuid, so letting the repository pick a different one would produce
    /// rows that cannot be decrypted.
    #[allow(clippy::too_many_arguments)]
    async fn store_certificate_version(
        &self,
        tenant_id: i64,
        certificate_version_uuid: &str,
        order_id: &str,
        version_no: i64,
        serial_sha256: &str,
        fingerprint_sha256: &str,
        spki_sha256: &str,
        chain_sha256: &str,
        issuer: &str,
        subject: &str,
        key_algorithm: &str,
        not_before: &str,
        not_after: &str,
        secret_bundle_ref: &str,
        material: &[SealedCertificateFile],
    ) -> DeployServiceResult<CertificateOrderResponse>;

    /// Reads back the sealed material of one certificate version.
    ///
    /// Returns the files still sealed; opening them is the service's job, so the
    /// repository never needs to hold, or be trusted with, the custody key.
    async fn retrieve_certificate_material(
        &self,
        tenant_id: i64,
        certificate_version_uuid: &str,
    ) -> DeployServiceResult<Vec<SealedCertificateFile>>;

    async fn retrieve_certificate_order(
        &self,
        tenant_id: i64,
        order_id: &str,
    ) -> DeployServiceResult<CertificateOrderResponse>;

    async fn list_certificate_orders(
        &self,
        tenant_id: i64,
        certificate_id: &str,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<CertificateOrderPage>;

    async fn list_certificate_challenges(
        &self,
        tenant_id: i64,
        order_id: &str,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<CertificateChallengePage>;

    /// Claims due certificates for renewal, opening one renewal attempt each.
    ///
    /// Claiming is where duplicate issuance is prevented, so it is deliberately a
    /// single repository call rather than a read followed by a write: the lease
    /// and the "at most one open attempt per certificate" unique index have to be
    /// taken in the same transaction as the choice of candidates, or two workers
    /// can both believe they own a certificate. A certificate another worker
    /// already holds is skipped silently, not reported as an error.
    ///
    /// The candidates are narrowed by the conditions that are *necessary* for
    /// renewal to be due; whether it actually is due is decided by
    /// [`crate::certificate_renewal`], so the window rule has exactly one
    /// implementation.
    async fn claim_due_certificate_renewals(
        &self,
        worker_id: &str,
        batch_size: i64,
        lease_seconds: i64,
        now: &str,
    ) -> DeployServiceResult<Vec<CertificateRenewalClaim>>;

    /// Claims certificate orders that are ready to make progress.
    ///
    /// Same single-statement discipline as the renewal claim above, for the same
    /// reason: choosing candidates and taking their lease must happen in one
    /// statement, or two workers can both conclude they own the same order and the
    /// CA is asked to issue twice for one intent.
    ///
    /// An order is claimable when it is non-terminal, past its `next_attempt_at`
    /// backoff, inside its deadline, and either unleased or past lease expiry — so
    /// a worker that dies mid-issuance is recovered by a later tick instead of
    /// stranding the order forever. `attempt_count` is charged here rather than by
    /// the caller, which is what makes bounded retries enforceable without a
    /// second read.
    async fn claim_certificate_orders(
        &self,
        worker_id: &str,
        batch_size: i64,
        lease_seconds: i64,
        now: &str,
    ) -> DeployServiceResult<Vec<CertificateOrderClaim>>;

    /// Advances an order, but only while `lease_owner` still holds a live lease
    /// on it.
    ///
    /// This is the fence PLAN-2026-0003 §9 asks for, and it is separate from
    /// [`advance_certificate_order`](Self::advance_certificate_order) because the
    /// two answer different questions. The unfenced form is an operator command:
    /// an authenticated caller may advance an order they are looking at. This one
    /// is a *worker* step, and a worker is only entitled to act on work whose lease
    /// it still holds — otherwise a worker that stalled past its lease would keep
    /// driving an order that a second worker had already taken over, and the CA
    /// would be asked to issue twice for one intent.
    ///
    /// Returns the status the order is in *after* the attempt: `to_status` when the
    /// transition landed, and otherwise whatever the order had already been moved
    /// to. A caller that does not see `to_status` has lost the order and must leave
    /// it alone rather than failing work another worker is actively doing.
    async fn advance_leased_certificate_order(
        &self,
        tenant_id: i64,
        order_id: &str,
        lease_owner: &str,
        from_status: &str,
        to_status: &str,
    ) -> DeployServiceResult<String>;

    /// Fails non-terminal orders whose `deadline_at` has passed.
    ///
    /// Without this an order that is neither claimable (past its deadline) nor
    /// terminal sits in the list forever, and an operator sees a request that never
    /// resolves. The deadline is the promise the control plane made when it accepted
    /// the request, so passing it is a failure to record — bounded per call, and
    /// `FOR UPDATE SKIP LOCKED` so two sweepers do not fight.
    async fn fail_expired_certificate_orders(
        &self,
        now: &str,
        batch_size: i64,
    ) -> DeployServiceResult<i64>;

    /// Records that a claimed renewal now has an order behind it.
    ///
    /// Separates `PLANNED` from `ORDERED` so a worker that dies between claiming
    /// and ordering is distinguishable from one that never claimed anything.
    async fn mark_certificate_renewal_ordered(
        &self,
        tenant_id: i64,
        renewal_uuid: &str,
        order_uuid: &str,
    ) -> DeployServiceResult<()>;

    /// Closes a renewal attempt that could not be ordered, and schedules the
    /// retry.
    ///
    /// The backoff is computed inside from the certificate's failure count rather
    /// than passed in, so the delay and the recorded severity cannot drift apart.
    async fn fail_certificate_renewal(
        &self,
        tenant_id: i64,
        renewal_uuid: &str,
        error_code: &str,
    ) -> DeployServiceResult<()>;

    /// Marks certificates and versions whose validity window has closed.
    ///
    /// A certificate that is still reported `ACTIVE` after `notAfter` is worse
    /// than one reported expired: every reader — the console, an alert rule, an
    /// operator deciding whether to intervene — treats it as working. Open renewal
    /// attempts are cancelled in the same pass, because an attempt to replace a
    /// certificate that expired while the scheduler was down is no longer the
    /// right work.
    async fn sweep_expired_certificates(
        &self,
        batch_size: i64,
        now: &str,
    ) -> DeployServiceResult<ExpiredCertificateSweep>;

    async fn list_certificate_renewals(
        &self,
        tenant_id: i64,
        certificate_id: &str,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<CertificateRenewalPage>;

    async fn run_retention(
        &self,
        dry_run: bool,
        package_retention_days: i64,
        release_retention_days: i64,
        build_log_retention_days: i64,
    ) -> DeployServiceResult<RetentionRunResponse>;

    async fn rebuild_usage_daily(
        &self,
        window_start: Option<&str>,
        window_end: Option<&str>,
    ) -> DeployServiceResult<UsageReconciliationResponse>;

    async fn list_signing_identity_health(
        &self,
        tenant_id: Option<i64>,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<SigningIdentityHealthPage>;

    async fn create_app_environment(
        &self,
        tenant_id: i64,
        actor_id: Option<i64>,
        app_id: &str,
        request: &CreateAppEnvironmentRequest,
    ) -> DeployServiceResult<AppEnvironmentResponse>;

    async fn list_app_environments(
        &self,
        tenant_id: i64,
        app_id: &str,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<AppEnvironmentPage>;

    async fn retrieve_app_environment(
        &self,
        tenant_id: i64,
        app_id: &str,
        environment_id: &str,
    ) -> DeployServiceResult<AppEnvironmentResponse>;

    async fn update_app_environment(
        &self,
        tenant_id: i64,
        actor_id: Option<i64>,
        app_id: &str,
        environment_id: &str,
        request: &UpdateAppEnvironmentRequest,
    ) -> DeployServiceResult<AppEnvironmentResponse>;

    async fn promote_environment(
        &self,
        tenant_id: i64,
        actor_id: Option<i64>,
        app_id: &str,
        environment_id: &str,
        request: &PromoteEnvironmentRequest,
    ) -> DeployServiceResult<EnvironmentPromotionResponse>;

    async fn list_environment_promotions(
        &self,
        tenant_id: i64,
        app_id: &str,
        environment_id: &str,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<EnvironmentPromotionPage>;

    async fn match_repository_by_url(
        &self,
        clone_url: &str,
    ) -> DeployServiceResult<Option<RepositoryMatch>>;

    async fn list_trigger_targets(&self, app_id: &str) -> DeployServiceResult<Vec<TriggerTarget>>;

    #[allow(clippy::too_many_arguments)]
    async fn ingest_source_event(
        &self,
        matched: &RepositoryMatch,
        event_kind: &str,
        source_ref: &str,
        source_commit: &str,
        commit_message: Option<&str>,
        sender_ref: Option<&str>,
        payload_sha256: &str,
    ) -> DeployServiceResult<(SourceEventResponse, bool)>;

    async fn update_source_event_result(
        &self,
        tenant_id: i64,
        event_id: &str,
        processed: bool,
        builds_triggered: i32,
        error_code: Option<&str>,
    ) -> DeployServiceResult<()>;

    async fn list_source_events(
        &self,
        tenant_id: Option<i64>,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<SourceEventPage>;

    // -- app publishing domains -------------------------------------------------

    /// The app's `appDomainSuffixes` override, or `None` when the app uses the
    /// platform catalog.
    async fn app_domain_suffix_override(
        &self,
        tenant_id: i64,
        app_id: &str,
    ) -> DeployServiceResult<Option<Vec<String>>>;

    /// Create the platform app-domain DNS zones for a tenant (idempotent);
    /// returns the number of newly created zones. `suffixes` selects the
    /// catalog to cover (the app's effective suffix set).
    async fn ensure_platform_app_zones(
        &self,
        tenant_id: i64,
        organization_id: i64,
        actor_id: Option<i64>,
        suffixes: &[String],
    ) -> DeployServiceResult<usize>;

    /// Reconcile an app's default publishing domains
    /// (`<appDomainLabel>.app[-<env>].<suffix>` domains + bindings) for one
    /// lifecycle environment. `app_id` is the app's public uuid; the label and
    /// suffix catalog are read from the app row. Auto-provisioned bindings
    /// that fall outside the effective catalog are retired in the same
    /// transaction.
    async fn provision_app_default_domains(
        &self,
        tenant_id: i64,
        organization_id: i64,
        actor_id: Option<i64>,
        app_id: &str,
        environment: &str,
    ) -> DeployServiceResult<ProvisionAppDomainsResult>;

    /// Resolve an active app binding by its exact hostname in one lifecycle
    /// environment and return that environment's newest VALID compiled runtime
    /// descriptor together with the app's nginx configuration (Web Server
    /// fallback lookup).
    async fn resolve_server_by_hostname(
        &self,
        hostname: &str,
        environment: &str,
    ) -> DeployServiceResult<Option<ResolvedDeployServer>>;

    // -- app template marketplace ---------------------------------------------
    //
    // Tenant scoping follows the module-wide rule: app-side callers pass a hard
    // `tenant_id`, admin callers pass `Option<i64>` where `None` is the
    // cross-tenant scope the backend console already uses elsewhere. Optional
    // `author_user_id` / `buyer_user_id` gates are the user-private ownership
    // filters (`IAM_SPEC.md` 5.1); `None` means "no ownership gate", which only
    // the marketplace browse surface and the admin surface may use.

    async fn list_template_categories(
        &self,
        tenant_id: Option<i64>,
        include_disabled: bool,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<TemplateCategoryPage>;

    async fn create_template_category(
        &self,
        tenant_id: i64,
        organization_id: Option<i64>,
        actor_id: Option<i64>,
        request: &CreateTemplateCategoryRequest,
    ) -> DeployServiceResult<TemplateCategoryResponse>;

    async fn retrieve_template_category(
        &self,
        tenant_id: Option<i64>,
        category_uuid: &str,
    ) -> DeployServiceResult<TemplateCategoryResponse>;

    async fn update_template_category(
        &self,
        tenant_id: i64,
        actor_id: Option<i64>,
        category_uuid: &str,
        request: &UpdateTemplateCategoryRequest,
    ) -> DeployServiceResult<TemplateCategoryResponse>;

    async fn delete_template_category(
        &self,
        tenant_id: i64,
        category_uuid: &str,
    ) -> DeployServiceResult<()>;

    /// The tenant marketplace: `PUBLIC` + `PUBLISHED` listings only. This is
    /// the one surface allowed to read without an ownership gate, and it is
    /// still fenced to one tenant.
    async fn list_marketplace_templates(
        &self,
        tenant_id: i64,
        query: &ListMarketplaceTemplatesQuery,
    ) -> DeployServiceResult<AppTemplateSummaryPage>;

    /// The public read of a marketplace listing; refuses non-`PUBLIC` /
    /// non-`PUBLISHED` rows as not found so a private listing is not
    /// distinguishable from a missing one.
    async fn retrieve_marketplace_template(
        &self,
        tenant_id: i64,
        template_uuid: &str,
    ) -> DeployServiceResult<AppTemplateResponse>;

    /// Ownership/marketplace state of one tenant listing, used to choose
    /// between the author read and the marketplace read *before* any body is
    /// read. `None` when the uuid does not resolve inside the tenant or is
    /// soft-deleted.
    async fn template_read_scope(
        &self,
        tenant_id: i64,
        template_uuid: &str,
    ) -> DeployServiceResult<Option<TemplateReadScope>>;

    /// Published versions of a marketplace-visible listing, for a consumer
    /// installing someone else's template.
    ///
    /// The listing must be `PUBLIC` + `PUBLISHED` and not deleted (the same
    /// gate as the marketplace shelf); an invisible listing is refused as not
    /// found rather than answered with an empty page, and draft/withdrawn
    /// versions never appear.
    async fn list_marketplace_template_versions(
        &self,
        tenant_id: i64,
        template_uuid: &str,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<AppTemplateVersionPage>;

    /// Increments the marketplace install counter after a successful install.
    async fn record_app_template_install(
        &self,
        tenant_id: i64,
        template_uuid: &str,
    ) -> DeployServiceResult<()>;

    /// Asserts a consumer may read one artifact because a published version of
    /// a marketplace-visible listing references it. Anything else — a draft
    /// version, a private or disabled listing — is refused as not found.
    async fn assert_marketplace_artifact_readable(
        &self,
        tenant_id: i64,
        artifact_uuid: &str,
    ) -> DeployServiceResult<()>;

    async fn list_app_templates(
        &self,
        tenant_id: i64,
        author_user_id: Option<i64>,
        query: &ListAppTemplatesQuery,
    ) -> DeployServiceResult<AppTemplatePage>;

    async fn list_app_templates_admin(
        &self,
        tenant_id: Option<i64>,
        query: &ListAppTemplatesAdminQuery,
    ) -> DeployServiceResult<AppTemplatePage>;

    /// `author_user_id = Some` scopes the read to that author's own listing;
    /// `None` is the admin/marketplace shape (tenant-scoped, no owner gate).
    async fn retrieve_app_template(
        &self,
        tenant_id: Option<i64>,
        author_user_id: Option<i64>,
        template_uuid: &str,
    ) -> DeployServiceResult<AppTemplateResponse>;

    async fn create_app_template(
        &self,
        tenant_id: i64,
        organization_id: Option<i64>,
        actor_id: Option<i64>,
        request: &CreateAppTemplateRequest,
    ) -> DeployServiceResult<AppTemplateResponse>;

    async fn update_app_template(
        &self,
        tenant_id: i64,
        author_user_id: i64,
        template_uuid: &str,
        request: &UpdateAppTemplateRequest,
    ) -> DeployServiceResult<AppTemplateResponse>;

    async fn delete_app_template(
        &self,
        tenant_id: Option<i64>,
        author_user_id: Option<i64>,
        template_uuid: &str,
    ) -> DeployServiceResult<()>;

    async fn submit_app_template(
        &self,
        tenant_id: i64,
        author_user_id: i64,
        template_uuid: &str,
    ) -> DeployServiceResult<AppTemplateResponse>;

    async fn list_app_template_versions(
        &self,
        tenant_id: Option<i64>,
        author_user_id: Option<i64>,
        template_uuid: &str,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<AppTemplateVersionPage>;

    async fn retrieve_app_template_version(
        &self,
        tenant_id: Option<i64>,
        author_user_id: Option<i64>,
        template_uuid: &str,
        version_uuid: &str,
    ) -> DeployServiceResult<AppTemplateVersionResponse>;

    async fn create_app_template_version(
        &self,
        tenant_id: i64,
        author_user_id: i64,
        template_uuid: &str,
        request: &CreateAppTemplateVersionRequest,
    ) -> DeployServiceResult<AppTemplateVersionResponse>;

    /// Backend moderation: moves `status` / `review_note` / `is_featured`. An
    /// approval publishes the newest draft version; see the service layer for
    /// the transition rules.
    async fn review_app_template(
        &self,
        tenant_id: Option<i64>,
        operator_id: Option<i64>,
        template_uuid: &str,
        request: &UpdateAppTemplateAdminRequest,
    ) -> DeployServiceResult<AppTemplateResponse>;

    // -- download catalog ---------------------------------------------------
    //
    // The catalog, its cards/items and the per-installer update configuration
    // replace the Cloud Router `downloads` site-settings document. Reads use
    // `Option<i64>` where `None` is the operator's cross-tenant scope; writes
    // require a concrete tenant.

    /// Lists the caller's catalogs without their nested documents.
    async fn list_download_catalogs(
        &self,
        tenant_id: Option<i64>,
    ) -> DeployServiceResult<Vec<DownloadCatalogSummaryDto>>;

    /// Reads one catalog with cards, items and update configurations.
    async fn retrieve_download_catalog(
        &self,
        tenant_id: Option<i64>,
        catalog_key: &str,
    ) -> DeployServiceResult<DownloadCatalogDto>;

    /// Creates the catalog when absent and patches the supplied fields otherwise.
    async fn upsert_download_catalog(
        &self,
        tenant_id: i64,
        operator_id: Option<i64>,
        catalog_key: &str,
        request: &UpsertDownloadCatalogRequest,
    ) -> DeployServiceResult<DownloadCatalogDto>;

    /// Bumps the revision and makes the catalog publicly readable.
    async fn publish_download_catalog(
        &self,
        tenant_id: i64,
        operator_id: Option<i64>,
        catalog_key: &str,
    ) -> DeployServiceResult<DownloadCatalogDto>;

    /// Creates or patches one card of a catalog.
    async fn upsert_download_card(
        &self,
        tenant_id: i64,
        operator_id: Option<i64>,
        catalog_key: &str,
        card_key: &str,
        request: &UpsertDownloadCardRequest,
    ) -> DeployServiceResult<DownloadCardDto>;

    /// Soft-deletes one card and its items.
    async fn delete_download_card(
        &self,
        tenant_id: i64,
        catalog_key: &str,
        card_key: &str,
    ) -> DeployServiceResult<()>;

    /// Creates or patches one installer of a card.
    async fn upsert_download_item(
        &self,
        tenant_id: i64,
        operator_id: Option<i64>,
        catalog_key: &str,
        card_key: &str,
        item_key: &str,
        request: &UpsertDownloadItemRequest,
    ) -> DeployServiceResult<DownloadItemDto>;

    /// Soft-deletes one installer and its update configuration.
    async fn delete_download_item(
        &self,
        tenant_id: i64,
        catalog_key: &str,
        card_key: &str,
        item_key: &str,
    ) -> DeployServiceResult<()>;

    /// Creates or replaces one installer's update configuration.
    async fn upsert_download_update(
        &self,
        tenant_id: i64,
        operator_id: Option<i64>,
        catalog_key: &str,
        card_key: &str,
        item_key: &str,
        request: &UpsertDownloadUpdateRequest,
    ) -> DeployServiceResult<DownloadUpdateDto>;

    /// Published catalog for the anonymous download page (unique key enforced by
    /// the service when two tenants publish the same key).
    async fn published_download_catalogs(
        &self,
        catalog_key: &str,
    ) -> DeployServiceResult<Vec<DownloadCatalogDto>>;

    /// Published installer resolved by product/platform/architecture.
    async fn published_download_record(
        &self,
        product_key: &str,
        platform: &str,
        architecture: &str,
    ) -> DeployServiceResult<Option<PublishedDownloadRecord>>;

    /// Published installer resolved by its stable public uuid.
    async fn published_download_record_by_item(
        &self,
        item_uuid: &str,
    ) -> DeployServiceResult<Option<PublishedDownloadRecord>>;

    /// Reads the application's remix source, with readiness derived from the
    /// current state of the repository/artifact it references.
    ///
    /// Never fails for "no source configured": an application without a source is
    /// a `MISSING` answer, not a missing resource. The application itself must
    /// exist in the tenant, which is where `NotFound` comes from.
    async fn retrieve_app_source(
        &self,
        tenant_id: i64,
        app_id: &str,
    ) -> DeployServiceResult<AppSourceResponse>;

    /// Creates or replaces the application's remix source.
    ///
    /// One transaction: a Git source also writes the reserved `remix`
    /// `deploy_source_repository` row, an archive source binds the artifact. The
    /// command carries already-normalized values — the service validated the
    /// provider, URL, ref, sub-path and clone mode — so this layer never re-decides
    /// what a valid source is.
    async fn update_app_source(
        &self,
        tenant_id: i64,
        actor_id: Option<i64>,
        app_id: &str,
        command: &UpdateAppSourceCommand,
    ) -> DeployServiceResult<AppSourceResponse>;

    /// Retires the application's remix source and turns `remix_enabled` off in the
    /// same transaction.
    ///
    /// Removing the source cannot leave remix enabled: the download would have
    /// nothing to resolve, and the flag would keep advertising a capability the
    /// application no longer has.
    async fn delete_app_source(
        &self,
        tenant_id: i64,
        actor_id: Option<i64>,
        app_id: &str,
    ) -> DeployServiceResult<()>;

    /// Writes `deploy_app.remix_enabled`.
    ///
    /// The caller has already proven the precondition (readiness) against the same
    /// tenant and application; this layer only persists the flag.
    async fn update_app_remix_enabled(
        &self,
        tenant_id: i64,
        actor_id: Option<i64>,
        app_id: &str,
        enabled: bool,
    ) -> DeployServiceResult<()>;

    /// Records the outcome of one Git validation attempt on the reserved `remix`
    /// repository row, and answers the re-read source.
    ///
    /// `repo_status`, `last_validated_at` and `last_error_code` are written
    /// together because they are one fact: `last_validated_at` is set exactly when
    /// the status is `VALIDATED`, and cleared otherwise, so a reader never has to
    /// decide which of two disagreeing columns to believe.
    ///
    /// Fails with `NotFound` when the application has no Git source to validate —
    /// an archive source is not a Git repository, and "nothing to probe" is not a
    /// successful validation.
    async fn record_git_repository_validation(
        &self,
        tenant_id: i64,
        actor_id: Option<i64>,
        app_id: &str,
        write: &GitRepositoryValidationWrite,
    ) -> DeployServiceResult<AppSourceResponse>;
}

/// A validated, normalized remix-source write.
///
/// An internal command rather than the wire request: by the time the repository
/// sees it, the provider is supported, the URL is acceptable, the ref and sub-path
/// are normalized, and the clone mode is defaulted. Keeping the wire DTO out of the
/// port means the repository cannot be handed a half-validated request by any
/// future caller.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpdateAppSourceCommand {
    pub kind: AppSourceKind,
    /// Present exactly when `kind = GIT`.
    pub git: Option<UpdateAppSourceGitCommand>,
    /// Present exactly when `kind = ARCHIVE`.
    pub archive_artifact_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpdateAppSourceGitCommand {
    pub repo_provider: String,
    pub repo_url: String,
    /// Normalized ref; `None` means the repository's default branch.
    pub repo_ref: Option<String>,
    /// Normalized repository-relative directory; `None` means the root.
    pub sub_path: Option<String>,
    pub credential_secret_ref: Option<String>,
    /// `FULL` or `SHALLOW`; already defaulted.
    pub clone_mode: String,
}

/// One recorded Git validation attempt.
///
/// The vocabulary comes from `sdkwork_deploy_core` rather than from strings, so a
/// repository cannot persist a status the readiness rule does not understand. The
/// timestamp is not carried: the writer stamps `last_validated_at` exactly for
/// `Validated`, which is what keeps the two columns consistent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GitRepositoryValidationWrite {
    pub repo_status: GitRepositoryValidationStatus,
    pub last_error_code: Option<GitValidationErrorCode>,
}

/// One published installer with the release facts a feed needs.
#[derive(Clone, Debug, Default)]
pub struct PublishedDownloadRecord {
    pub catalog: DownloadCatalogDto,
    pub item: DownloadItemDto,
    pub update: Option<DownloadUpdateDto>,
    pub package: Option<PublishedDownloadPackage>,
}

/// Release-domain package facts joined onto a published installer.
#[derive(Clone, Debug, Default)]
pub struct PublishedDownloadPackage {
    pub version: String,
    pub file_name: String,
    pub size_bytes: i64,
    pub sha512: Option<String>,
    pub drive_node_id: Option<String>,
    pub drive_space_id: Option<String>,
    pub release_notes_url: Option<String>,
}
