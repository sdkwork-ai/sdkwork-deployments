//! Repository port consumed by the deploy service layer.

use async_trait::async_trait;
use sdkwork_deploy_contract::DeployServiceResult;
use sdkwork_deploy_contract::{
    AcmeAccountPage, AcmeAccountResponse, AppDatabaseMigrationPage, AppDatabaseMigrationResponse,
    AppDatabaseProfilePage, AppDatabaseProfileResponse, AppDeploymentPage, AppDeploymentResponse,
    AppEnvironmentPage, AppEnvironmentResponse, AppPage, AppReleasePage, AppReleaseResponse,
    AppResponse, ArtifactPage, ArtifactResponse, AuditLogPage, BuildPage, BuildQueuePage,
    BuildResponse, BuildTemplatePage, BuildTemplateResponse, CertificateChallengePage,
    CertificateOrderPage, CertificateOrderResponse, CertificatePage, CertificateResponse,
    ChannelPage, ChannelResponse, ChannelRolloutPage, ChannelRolloutResponse,
    CreateAcmeAccountRequest, CreateAppDatabaseMigrationRequest, CreateAppDatabaseProfileRequest,
    CreateAppDeploymentRequest, CreateAppEnvironmentRequest, CreateAppReleaseRequest,
    CreateAppRequest, CreateArtifactRequest, CreateBuildRequest, CreateBuildTemplateRequest,
    CreateCertificateRequest, CreateDeployUploadSessionRequest, CreateDomainHostnameRequest,
    CreateDomainZoneRequest, CreateEnvVariableRequest, CreateHealthCheckRequest,
    CreateNginxConfigRequest, CreateNodeClusterRequest, CreatePlatformTargetRequest,
    CreateServerRequest, CreateSigningIdentityRequest, CreateSourceRepositoryRequest,
    DeployAppRequestContext, DeployUploadSessionResponse, DeploymentStatus, DomainHostnamePage,
    DomainHostnameResponse, DomainZonePage, DomainZoneResponse, EntitlementProjectionPage,
    EnvVariablePage, EnvVariableResponse, EnvironmentPromotionPage, EnvironmentPromotionResponse,
    HealthCheckPage, HealthCheckResponse, ListDomainZonesQuery, ListNginxConfigsQuery,
    NginxConfigPage, NginxConfigResponse, NginxReloadResponse, NginxStatusResponse,
    NginxValidateResponse, NodeClusterPage, NodeClusterResponse, PackagePage, PackageResponse,
    PlatformTargetPage, PlatformTargetResponse, PromoteChannelRequest, PromoteEnvironmentRequest,
    ProvisionAppDomainsResult, RegisterPackageRequest, ReleaseStatus,
    RequestCertificateOrderRequest, ResolvedDeployServer, RetentionRunResponse, RunnerHealthPage,
    ServerPage, ServerResponse, SigningIdentityHealthPage, SigningIdentityPage,
    SigningIdentityResponse, SourceEventPage, SourceEventResponse, SourceRepositoryPage,
    SourceRepositoryResponse, UpdateAppDatabaseProfileRequest, UpdateAppEnvironmentRequest,
    UpdateAppRequest, UpdateBuildStateRequest, UpdateDomainZoneRequest, UpdateNginxConfigRequest,
    UpdateNodeClusterRequest, UpdateServerRequest, UsageEventPage, UsageEventResponse,
    UsageReconciliationResponse,
};

use crate::DomainVerificationChallenge;
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

#[async_trait]
pub trait DeployRepositoryPort: crate::AppCompositionRepositoryPort + Send + Sync {
    async fn ready_check(&self) -> DeployServiceResult<()>;

    async fn list_domain_zones(
        &self,
        tenant_id: i64,
        query: &ListDomainZonesQuery,
    ) -> DeployServiceResult<DomainZonePage>;

    async fn create_domain_zone(
        &self,
        tenant_id: i64,
        organization_id: Option<i64>,
        actor_id: Option<i64>,
        request: &CreateDomainZoneRequest,
    ) -> DeployServiceResult<DomainZoneResponse>;

    async fn retrieve_domain_zone(
        &self,
        tenant_id: i64,
        zone_id: &str,
    ) -> DeployServiceResult<DomainZoneResponse>;

    async fn update_domain_zone(
        &self,
        tenant_id: i64,
        actor_id: Option<i64>,
        zone_id: &str,
        request: &UpdateDomainZoneRequest,
    ) -> DeployServiceResult<DomainZoneResponse>;

    async fn delete_domain_zone(&self, tenant_id: i64, zone_id: &str) -> DeployServiceResult<()>;

    async fn list_domain_hostnames(
        &self,
        tenant_id: i64,
        zone_id: &str,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<DomainHostnamePage>;

    async fn create_domain_hostname(
        &self,
        tenant_id: i64,
        actor_id: Option<i64>,
        zone_id: &str,
        request: &CreateDomainHostnameRequest,
    ) -> DeployServiceResult<DomainHostnameResponse>;

    async fn retrieve_domain_hostname(
        &self,
        tenant_id: i64,
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

    async fn delete_domain_hostname(
        &self,
        tenant_id: i64,
        zone_id: &str,
        hostname_id: &str,
    ) -> DeployServiceResult<()>;

    async fn domain_hostname_verification_challenge(
        &self,
        tenant_id: i64,
        zone_id: &str,
        hostname_id: &str,
    ) -> DeployServiceResult<DomainVerificationChallenge>;

    async fn confirm_domain_hostname_verification(
        &self,
        tenant_id: i64,
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

    async fn list_artifacts(
        &self,
        tenant_id: i64,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<ArtifactPage>;

    async fn create_artifact_from_drive(
        &self,
        tenant_id: i64,
        request: &CreateArtifactRequest,
    ) -> DeployServiceResult<ArtifactResponse>;

    async fn retrieve_artifact(
        &self,
        tenant_id: i64,
        artifact_id: &str,
    ) -> DeployServiceResult<ArtifactResponse>;

    async fn retain_artifact(&self, tenant_id: i64, artifact_id: &str) -> DeployServiceResult<()>;

    async fn create_artifact_from_upload_session(
        &self,
        tenant_id: i64,
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

    async fn create_upload_session_ref(
        &self,
        tenant_id: i64,
        context: &DeployAppRequestContext,
        request: &CreateDeployUploadSessionRequest,
        drive: &DeployUploadSessionResponse,
    ) -> DeployServiceResult<DeployUploadSessionResponse>;

    async fn find_upload_session_by_idempotency_key(
        &self,
        tenant_id: i64,
        idempotency_key: &str,
    ) -> DeployServiceResult<Option<DeployUploadSessionResponse>>;

    async fn retrieve_upload_session_ref(
        &self,
        tenant_id: i64,
        upload_session_id: &str,
    ) -> DeployServiceResult<DeployUploadSessionResponse>;

    async fn update_upload_session_status(
        &self,
        tenant_id: i64,
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
        request: &CreateAppRequest,
    ) -> DeployServiceResult<AppResponse>;

    async fn list_apps(
        &self,
        tenant_id: i64,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<AppPage>;

    async fn retrieve_app(&self, tenant_id: i64, app_id: &str) -> DeployServiceResult<AppResponse>;

    async fn update_app(
        &self,
        tenant_id: i64,
        actor_id: Option<i64>,
        app_id: &str,
        request: &UpdateAppRequest,
    ) -> DeployServiceResult<AppResponse>;

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

    #[allow(clippy::too_many_arguments)]
    async fn store_certificate_version(
        &self,
        tenant_id: i64,
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
    ) -> DeployServiceResult<CertificateOrderResponse>;

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
}
