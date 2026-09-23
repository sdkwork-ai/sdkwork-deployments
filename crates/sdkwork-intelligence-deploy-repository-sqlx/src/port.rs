//! `DeployRepositoryPort` trait implementation delegating to SQLx repository modules.

use async_trait::async_trait;
use sdkwork_deploy_certificate_material::SealedCertificateFile;
use sdkwork_deploy_contract::{
    AcmeAccountPage, AcmeAccountResponse, AppDatabaseMigrationPage, AppDatabaseMigrationResponse,
    AppDatabaseProfilePage, AppDatabaseProfileResponse, AppDeploymentPage, AppDeploymentResponse,
    AppDomainPage, AppEnvironmentPage, AppEnvironmentResponse, AppPage, AppReleasePage,
    AppReleaseResponse, AppResponse, ArtifactPage, ArtifactResponse, AuditLogPage, BuildPage,
    BuildQueuePage, BuildResponse, BuildTemplatePage, BuildTemplateResponse,
    CertificateChallengePage, CertificateOrderPage, CertificateOrderResponse, CertificatePage,
    CertificateRenewalPage, CertificateResponse, ChannelPage, ChannelResponse, ChannelRolloutPage,
    ChannelRolloutResponse, CreateAcmeAccountRequest, CreateAppDatabaseMigrationRequest,
    CreateAppDatabaseProfileRequest, CreateAppDeploymentRequest, CreateAppEnvironmentRequest,
    CreateAppReleaseRequest, CreateAppRequest, CreateArtifactRequest, CreateBuildRequest,
    CreateBuildTemplateRequest, CreateCertificateRequest, CreateDeployUploadSessionRequest,
    CreateDomainHostnameRequest, CreateDomainZoneRequest, CreateEnvVariableRequest,
    CreateHealthCheckRequest, CreateNginxConfigRequest, CreateNodeClusterRequest,
    CreatePlatformTargetRequest, CreateServerRequest, CreateSigningIdentityRequest,
    CreateSourceRepositoryRequest, DeployAppRequestContext, DeployUploadSessionResponse,
    DeploymentStatus, DomainHostnamePage, DomainHostnameResponse, DomainZonePage,
    DomainZoneResponse, EntitlementProjectionPage, EnvVariablePage, EnvVariableResponse,
    EnvironmentPromotionPage, EnvironmentPromotionResponse, HealthCheckPage, HealthCheckResponse,
    ListAppsQuery, ListDomainZonesQuery, ListNginxConfigsQuery, NginxConfigPage,
    NginxConfigResponse, NginxReloadResponse, NginxStatusResponse, NginxValidateResponse,
    NodeClusterPage, NodeClusterResponse, PackagePage, PackageResponse, PlatformTargetPage,
    PlatformTargetResponse, PromoteChannelRequest, PromoteEnvironmentRequest,
    RegisterPackageRequest, ReleaseStatus, RequestCertificateOrderRequest, RetentionRunResponse,
    RunnerHealthPage, ServerPage, ServerResponse, SigningIdentityHealthPage, SigningIdentityPage,
    SigningIdentityResponse, SourceEventPage, SourceEventResponse, SourceRepositoryPage,
    SourceRepositoryResponse, UpdateAppDatabaseProfileRequest, UpdateAppEnvironmentRequest,
    UpdateAppRequest, UpdateBuildStateRequest, UpdateDomainHostnameRequest,
    UpdateDomainZoneRequest, UpdateNginxConfigRequest, UpdateNodeClusterRequest,
    UpdateServerRequest, UsageEventPage, UsageEventResponse, UsageReconciliationResponse,
};
use sdkwork_deploy_contract::{
    DeployServiceError, DeployServiceResult, ProvisionAppDomainsResult, ResolvedDeployServer,
    UsageEventIngestItem, UsageEventQuery, UsageIngestResult,
};
use sdkwork_deploy_web_port::RuntimeAssignmentReceipt;
use sdkwork_intelligence_deploy_service::repository::{
    InsertAuditLogCommand, InsertUsageEventCommand,
};
use sdkwork_intelligence_deploy_service::repository::{RepositoryMatch, TriggerTarget};
use sdkwork_intelligence_deploy_service::runtime_publication::{
    DeployRuntimeAssignmentMutationPort, DeployRuntimeAssignmentRepositoryPort,
    RuntimeAssignmentState, RuntimeObservationEvidence, RuntimeObservationPersistenceResult,
};
use sdkwork_intelligence_deploy_service::{
    CertificateOrderCaaSubject, CertificateOrderClaim, CertificateRenewalClaim,
    DeployRepositoryPort, DomainVerificationChallenge, ExpiredCertificateSweep,
};

use crate::DeployRepository;

#[async_trait]
impl DeployRepositoryPort for DeployRepository {
    async fn ready_check(&self) -> DeployServiceResult<()> {
        sqlx::query("SELECT 1")
            .execute(&self.pool)
            .await
            .map_err(|_| DeployServiceError::DatabaseUnavailable)?;
        Ok(())
    }

    async fn list_domain_zones(
        &self,
        tenant_id: i64,
        owner_user_id: Option<i64>,
        query: &ListDomainZonesQuery,
    ) -> DeployServiceResult<DomainZonePage> {
        self.list_domain_zones_repo(tenant_id, owner_user_id, query)
            .await
    }

    async fn retrieve_dns_challenge_zone(
        &self,
        tenant_id: i64,
        hostname: &str,
    ) -> DeployServiceResult<Option<sdkwork_intelligence_deploy_service::DnsChallengeZone>> {
        self.retrieve_dns_challenge_zone_repo(tenant_id, hostname)
            .await
    }

    async fn create_domain_zone(
        &self,
        tenant_id: i64,
        organization_id: Option<i64>,
        actor_id: Option<i64>,
        request: &CreateDomainZoneRequest,
    ) -> DeployServiceResult<DomainZoneResponse> {
        self.create_domain_zone_repo(tenant_id, organization_id, actor_id, request)
            .await
    }

    async fn retrieve_domain_zone(
        &self,
        tenant_id: i64,
        owner_user_id: Option<i64>,
        zone_id: &str,
    ) -> DeployServiceResult<DomainZoneResponse> {
        self.retrieve_domain_zone_repo(tenant_id, owner_user_id, zone_id)
            .await
    }

    async fn update_domain_zone(
        &self,
        tenant_id: i64,
        actor_id: Option<i64>,
        zone_id: &str,
        request: &UpdateDomainZoneRequest,
    ) -> DeployServiceResult<DomainZoneResponse> {
        self.update_domain_zone_repo(tenant_id, actor_id, zone_id, request)
            .await
    }

    async fn delete_domain_zone(
        &self,
        tenant_id: i64,
        owner_user_id: Option<i64>,
        zone_id: &str,
    ) -> DeployServiceResult<()> {
        self.delete_domain_zone_repo(tenant_id, owner_user_id, zone_id)
            .await
    }

    async fn list_domain_hostnames(
        &self,
        tenant_id: i64,
        owner_user_id: Option<i64>,
        zone_id: &str,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<DomainHostnamePage> {
        self.list_domain_hostnames_repo(tenant_id, owner_user_id, zone_id, page, page_size)
            .await
    }

    async fn create_domain_hostname(
        &self,
        tenant_id: i64,
        actor_id: Option<i64>,
        zone_id: &str,
        request: &CreateDomainHostnameRequest,
    ) -> DeployServiceResult<DomainHostnameResponse> {
        self.create_domain_hostname_repo(tenant_id, actor_id, zone_id, request)
            .await
    }

    async fn ensure_domain_hostname(
        &self,
        tenant_id: i64,
        actor_id: Option<i64>,
        zone_id: &str,
        relative_name: &str,
    ) -> DeployServiceResult<DomainHostnameResponse> {
        self.ensure_domain_hostname_repo(tenant_id, actor_id, zone_id, relative_name)
            .await
    }

    async fn retrieve_domain_hostname(
        &self,
        tenant_id: i64,
        owner_user_id: Option<i64>,
        zone_id: &str,
        hostname_id: &str,
    ) -> DeployServiceResult<DomainHostnameResponse> {
        self.retrieve_domain_hostname_repo(tenant_id, owner_user_id, zone_id, hostname_id)
            .await
    }

    async fn delete_domain_hostname(
        &self,
        tenant_id: i64,
        owner_user_id: Option<i64>,
        zone_id: &str,
        hostname_id: &str,
    ) -> DeployServiceResult<()> {
        self.delete_domain_hostname_repo(tenant_id, owner_user_id, zone_id, hostname_id)
            .await
    }

    async fn update_domain_hostname(
        &self,
        tenant_id: i64,
        actor_id: Option<i64>,
        zone_id: &str,
        hostname_id: &str,
        request: &UpdateDomainHostnameRequest,
    ) -> DeployServiceResult<DomainHostnameResponse> {
        self.update_domain_hostname_repo(tenant_id, actor_id, zone_id, hostname_id, request)
            .await
    }

    async fn domain_hostname_verification_challenge(
        &self,
        tenant_id: i64,
        owner_user_id: Option<i64>,
        zone_id: &str,
        hostname_id: &str,
    ) -> DeployServiceResult<DomainVerificationChallenge> {
        self.domain_hostname_verification_challenge_repo(
            tenant_id,
            owner_user_id,
            zone_id,
            hostname_id,
        )
        .await
    }

    async fn confirm_domain_hostname_verification(
        &self,
        tenant_id: i64,
        owner_user_id: Option<i64>,
        zone_id: &str,
        hostname_id: &str,
        verification_id: &str,
        observed_sha256: &str,
        verifier_identity: &str,
    ) -> DeployServiceResult<bool> {
        self.confirm_domain_hostname_verification_repo(
            tenant_id,
            owner_user_id,
            zone_id,
            hostname_id,
            verification_id,
            observed_sha256,
            verifier_identity,
        )
        .await
    }

    async fn set_app_status(
        &self,
        tenant_id: i64,
        app_id: &str,
        status: i32,
    ) -> DeployServiceResult<AppResponse> {
        self.set_app_status_repo(tenant_id, app_id, status).await
    }

    async fn list_artifacts(
        &self,
        tenant_id: i64,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<ArtifactPage> {
        self.list_artifacts_repo(tenant_id, page, page_size).await
    }

    async fn create_artifact_from_drive(
        &self,
        tenant_id: i64,
        request: &CreateArtifactRequest,
    ) -> DeployServiceResult<ArtifactResponse> {
        self.create_artifact_from_drive_repo(tenant_id, request)
            .await
    }

    async fn retrieve_artifact(
        &self,
        tenant_id: i64,
        artifact_id: &str,
    ) -> DeployServiceResult<ArtifactResponse> {
        self.retrieve_artifact_repo(tenant_id, artifact_id).await
    }

    async fn retain_artifact(&self, tenant_id: i64, artifact_id: &str) -> DeployServiceResult<()> {
        self.retain_artifact_repo(tenant_id, artifact_id).await
    }

    async fn create_artifact_from_upload_session(
        &self,
        tenant_id: i64,
        upload_session_id: &str,
        checksum_sha256: &str,
    ) -> DeployServiceResult<ArtifactResponse> {
        self.create_artifact_from_upload_session_repo(tenant_id, upload_session_id, checksum_sha256)
            .await
    }

    async fn list_env_variables(
        &self,
        tenant_id: i64,
        app_id: &str,
        environment: Option<&str>,
    ) -> DeployServiceResult<EnvVariablePage> {
        self.list_env_variables_repo(tenant_id, app_id, environment)
            .await
    }

    async fn create_env_variable(
        &self,
        tenant_id: i64,
        app_id: &str,
        request: &CreateEnvVariableRequest,
    ) -> DeployServiceResult<EnvVariableResponse> {
        self.create_env_variable_repo(tenant_id, app_id, request)
            .await
    }

    async fn list_certificates(
        &self,
        tenant_id: i64,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<CertificatePage> {
        self.list_certificates_repo(tenant_id, page, page_size)
            .await
    }

    async fn create_certificate(
        &self,
        tenant_id: i64,
        organization_id: Option<i64>,
        actor_id: Option<i64>,
        idempotency_key: &str,
        request: &CreateCertificateRequest,
    ) -> DeployServiceResult<CertificateResponse> {
        self.create_certificate_repo(
            tenant_id,
            organization_id,
            actor_id,
            idempotency_key,
            request,
        )
        .await
    }

    async fn retrieve_certificate(
        &self,
        tenant_id: i64,
        certificate_id: &str,
    ) -> DeployServiceResult<CertificateResponse> {
        self.retrieve_certificate_repo(tenant_id, certificate_id)
            .await
    }

    async fn delete_certificate(
        &self,
        tenant_id: i64,
        certificate_id: &str,
    ) -> DeployServiceResult<()> {
        self.delete_certificate_repo(tenant_id, certificate_id)
            .await
    }

    async fn renew_certificate(
        &self,
        tenant_id: i64,
        certificate_id: &str,
    ) -> DeployServiceResult<CertificateResponse> {
        self.renew_certificate_repo(tenant_id, certificate_id).await
    }

    async fn list_health_checks(
        &self,
        tenant_id: i64,
        app_id: &str,
    ) -> DeployServiceResult<HealthCheckPage> {
        self.list_health_checks_repo(tenant_id, app_id).await
    }

    async fn create_health_check(
        &self,
        tenant_id: i64,
        app_id: &str,
        request: &CreateHealthCheckRequest,
    ) -> DeployServiceResult<HealthCheckResponse> {
        self.create_health_check_repo(tenant_id, app_id, request)
            .await
    }

    async fn list_nginx_configs(
        &self,
        tenant_id: Option<i64>,
        query: &ListNginxConfigsQuery,
    ) -> DeployServiceResult<NginxConfigPage> {
        self.list_nginx_configs_repo(tenant_id, query).await
    }

    async fn create_nginx_config(
        &self,
        tenant_id: i64,
        request: &CreateNginxConfigRequest,
    ) -> DeployServiceResult<NginxConfigResponse> {
        self.create_nginx_config_repo(tenant_id, request).await
    }

    async fn retrieve_nginx_config(
        &self,
        tenant_id: Option<i64>,
        config_id: &str,
    ) -> DeployServiceResult<NginxConfigResponse> {
        self.retrieve_nginx_config_repo(tenant_id, config_id).await
    }

    async fn update_nginx_config(
        &self,
        tenant_id: Option<i64>,
        config_id: &str,
        request: &UpdateNginxConfigRequest,
    ) -> DeployServiceResult<NginxConfigResponse> {
        self.update_nginx_config_repo(tenant_id, config_id, request)
            .await
    }

    async fn validate_nginx_config(
        &self,
        tenant_id: Option<i64>,
        config_id: &str,
    ) -> DeployServiceResult<NginxValidateResponse> {
        self.validate_nginx_config_repo(tenant_id, config_id).await
    }

    async fn deploy_nginx_config(
        &self,
        tenant_id: Option<i64>,
        config_id: &str,
    ) -> DeployServiceResult<NginxConfigResponse> {
        self.deploy_nginx_config_repo(tenant_id, config_id).await
    }

    async fn reload_nginx(&self) -> DeployServiceResult<NginxReloadResponse> {
        self.reload_nginx_repo().await
    }

    async fn retrieve_nginx_status(
        &self,
        tenant_id: Option<i64>,
    ) -> DeployServiceResult<NginxStatusResponse> {
        self.retrieve_nginx_status_repo(tenant_id).await
    }

    async fn list_servers(
        &self,
        tenant_id: i64,
        page: i32,
        page_size: i32,
        cluster_id: Option<String>,
    ) -> DeployServiceResult<ServerPage> {
        self.list_servers_repo(tenant_id, page, page_size, cluster_id)
            .await
    }

    async fn create_server(
        &self,
        tenant_id: i64,
        request: &CreateServerRequest,
    ) -> DeployServiceResult<ServerResponse> {
        self.create_server_repo(tenant_id, request).await
    }

    async fn update_server(
        &self,
        tenant_id: i64,
        server_id: &str,
        request: &UpdateServerRequest,
    ) -> DeployServiceResult<ServerResponse> {
        self.update_server_repo(tenant_id, server_id, request).await
    }

    async fn list_node_clusters(
        &self,
        tenant_id: i64,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<NodeClusterPage> {
        self.list_node_clusters_repo(tenant_id, page, page_size)
            .await
    }

    async fn create_node_cluster(
        &self,
        tenant_id: i64,
        request: &CreateNodeClusterRequest,
    ) -> DeployServiceResult<NodeClusterResponse> {
        self.create_node_cluster_repo(tenant_id, request).await
    }

    async fn update_node_cluster(
        &self,
        tenant_id: i64,
        cluster_id: &str,
        request: &UpdateNodeClusterRequest,
    ) -> DeployServiceResult<NodeClusterResponse> {
        self.update_node_cluster_repo(tenant_id, cluster_id, request)
            .await
    }

    async fn list_audit_logs(
        &self,
        tenant_id: Option<i64>,
        query: &sdkwork_deploy_contract::AuditLogQuery,
        cursor: Option<&str>,
    ) -> DeployServiceResult<AuditLogPage> {
        self.list_audit_logs_repo(tenant_id, query, cursor).await
    }

    async fn insert_audit_log(&self, command: InsertAuditLogCommand) -> DeployServiceResult<()> {
        self.insert_audit_log_repo(&command).await
    }

    async fn create_upload_session_ref(
        &self,
        tenant_id: i64,
        context: &DeployAppRequestContext,
        request: &CreateDeployUploadSessionRequest,
        drive: &DeployUploadSessionResponse,
    ) -> DeployServiceResult<DeployUploadSessionResponse> {
        self.create_upload_session_ref_repo(tenant_id, context, request, drive)
            .await
    }

    async fn find_upload_session_by_idempotency_key(
        &self,
        tenant_id: i64,
        idempotency_key: &str,
    ) -> DeployServiceResult<Option<DeployUploadSessionResponse>> {
        self.find_upload_session_by_idempotency_key_repo(tenant_id, idempotency_key)
            .await
    }

    async fn retrieve_upload_session_ref(
        &self,
        tenant_id: i64,
        upload_session_id: &str,
    ) -> DeployServiceResult<DeployUploadSessionResponse> {
        self.retrieve_upload_session_ref_repo(tenant_id, upload_session_id)
            .await
    }

    async fn update_upload_session_status(
        &self,
        tenant_id: i64,
        upload_session_id: &str,
        status: i32,
        drive_node_id: Option<&str>,
    ) -> DeployServiceResult<DeployUploadSessionResponse> {
        self.update_upload_session_status_repo(tenant_id, upload_session_id, status, drive_node_id)
            .await
    }

    // -- unified app delivery (REQ-2026-0002) --------------------------------

    async fn create_app(
        &self,
        tenant_id: i64,
        organization_id: Option<i64>,
        actor_id: Option<i64>,
        idempotency_key: Option<&str>,
        request: &CreateAppRequest,
    ) -> DeployServiceResult<AppResponse> {
        self.create_app_repo(
            tenant_id,
            organization_id,
            actor_id,
            idempotency_key,
            request,
        )
        .await
    }

    async fn list_apps(
        &self,
        tenant_id: i64,
        actor_id: Option<i64>,
        organization_id: Option<i64>,
        query: &ListAppsQuery,
    ) -> DeployServiceResult<AppPage> {
        self.list_apps_repo(tenant_id, actor_id, organization_id, query)
            .await
    }

    async fn retrieve_app(&self, tenant_id: i64, app_id: &str) -> DeployServiceResult<AppResponse> {
        self.retrieve_app_repo(tenant_id, app_id).await
    }

    async fn update_app(
        &self,
        tenant_id: i64,
        actor_id: Option<i64>,
        app_id: &str,
        request: &UpdateAppRequest,
    ) -> DeployServiceResult<AppResponse> {
        self.update_app_repo(tenant_id, actor_id, app_id, request)
            .await
    }

    async fn list_app_domains(
        &self,
        tenant_id: i64,
        app_id: &str,
    ) -> DeployServiceResult<AppDomainPage> {
        self.list_app_domains_repo(tenant_id, app_id).await
    }

    async fn create_platform_target(
        &self,
        tenant_id: i64,
        app_id: &str,
        actor_id: Option<i64>,
        request: &CreatePlatformTargetRequest,
    ) -> DeployServiceResult<PlatformTargetResponse> {
        self.create_platform_target_repo(tenant_id, app_id, actor_id, request)
            .await
    }

    async fn list_platform_targets(
        &self,
        tenant_id: i64,
        app_id: &str,
    ) -> DeployServiceResult<PlatformTargetPage> {
        self.list_platform_targets_repo(tenant_id, app_id).await
    }

    async fn retrieve_platform_target(
        &self,
        tenant_id: i64,
        app_id: &str,
        target_id: &str,
    ) -> DeployServiceResult<PlatformTargetResponse> {
        self.retrieve_platform_target_repo(tenant_id, app_id, target_id)
            .await
    }

    async fn create_source_repository(
        &self,
        tenant_id: i64,
        app_id: &str,
        actor_id: Option<i64>,
        request: &CreateSourceRepositoryRequest,
    ) -> DeployServiceResult<SourceRepositoryResponse> {
        self.create_source_repository_repo(tenant_id, app_id, actor_id, request)
            .await
    }

    async fn list_source_repositories(
        &self,
        tenant_id: i64,
        app_id: &str,
    ) -> DeployServiceResult<SourceRepositoryPage> {
        self.list_source_repositories_repo(tenant_id, app_id).await
    }

    async fn retrieve_source_repository(
        &self,
        tenant_id: i64,
        app_id: &str,
        repo_id: &str,
    ) -> DeployServiceResult<SourceRepositoryResponse> {
        self.retrieve_source_repository_repo(tenant_id, app_id, repo_id)
            .await
    }

    async fn create_build_template(
        &self,
        tenant_id: i64,
        actor_id: Option<i64>,
        request: &CreateBuildTemplateRequest,
    ) -> DeployServiceResult<BuildTemplateResponse> {
        self.create_build_template_repo(tenant_id, actor_id, request)
            .await
    }

    async fn list_build_templates(
        &self,
        tenant_id: i64,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<BuildTemplatePage> {
        self.list_build_templates_repo(tenant_id, page, page_size)
            .await
    }

    async fn retrieve_build_template(
        &self,
        tenant_id: i64,
        template_id: &str,
    ) -> DeployServiceResult<BuildTemplateResponse> {
        self.retrieve_build_template_repo(tenant_id, template_id)
            .await
    }

    async fn create_build(
        &self,
        tenant_id: i64,
        actor_id: Option<i64>,
        request: &CreateBuildRequest,
    ) -> DeployServiceResult<BuildResponse> {
        self.create_build_repo(tenant_id, actor_id, request).await
    }

    async fn list_builds(
        &self,
        tenant_id: i64,
        app_id: &str,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<BuildPage> {
        self.list_builds_repo(tenant_id, app_id, page, page_size)
            .await
    }

    async fn retrieve_build(
        &self,
        tenant_id: i64,
        app_id: &str,
        build_id: &str,
    ) -> DeployServiceResult<BuildResponse> {
        self.retrieve_build_repo(tenant_id, app_id, build_id).await
    }

    async fn update_build_state(
        &self,
        tenant_id: i64,
        app_id: &str,
        build_id: &str,
        request: &UpdateBuildStateRequest,
    ) -> DeployServiceResult<BuildResponse> {
        self.update_build_state_repo(tenant_id, app_id, build_id, request)
            .await
    }

    async fn claim_next_build(
        &self,
        tenant_id: i64,
        runner_node_uuid: &str,
        runner_version: &str,
    ) -> DeployServiceResult<Option<BuildResponse>> {
        self.claim_next_build_repo(tenant_id, runner_node_uuid, runner_version)
            .await
    }

    async fn resolve_build_platform(
        &self,
        tenant_id: i64,
        build_id: &str,
    ) -> DeployServiceResult<(String, String, String)> {
        self.resolve_build_platform_repo(tenant_id, build_id).await
    }

    async fn register_package(
        &self,
        tenant_id: i64,
        actor_id: Option<i64>,
        request: &RegisterPackageRequest,
    ) -> DeployServiceResult<PackageResponse> {
        self.register_package_repo(tenant_id, actor_id, request)
            .await
    }

    async fn list_packages(
        &self,
        tenant_id: i64,
        app_id: &str,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<PackagePage> {
        self.list_packages_repo(tenant_id, app_id, page, page_size)
            .await
    }

    async fn retrieve_package(
        &self,
        tenant_id: i64,
        app_id: &str,
        package_id: &str,
    ) -> DeployServiceResult<PackageResponse> {
        self.retrieve_package_repo(tenant_id, app_id, package_id)
            .await
    }

    async fn create_app_release(
        &self,
        tenant_id: i64,
        actor_id: Option<i64>,
        request: &CreateAppReleaseRequest,
    ) -> DeployServiceResult<AppReleaseResponse> {
        self.create_app_release_repo(tenant_id, actor_id, request)
            .await
    }

    async fn list_app_releases(
        &self,
        tenant_id: i64,
        app_id: &str,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<AppReleasePage> {
        self.list_app_releases_repo(tenant_id, app_id, page, page_size)
            .await
    }

    async fn retrieve_app_release(
        &self,
        tenant_id: i64,
        app_id: &str,
        release_id: &str,
    ) -> DeployServiceResult<AppReleaseResponse> {
        self.retrieve_app_release_repo(tenant_id, app_id, release_id)
            .await
    }

    async fn update_app_release_status(
        &self,
        tenant_id: i64,
        app_id: &str,
        release_id: &str,
        release_status: ReleaseStatus,
    ) -> DeployServiceResult<AppReleaseResponse> {
        self.update_app_release_status_repo(tenant_id, app_id, release_id, release_status)
            .await
    }

    async fn ensure_release_channel(
        &self,
        tenant_id: i64,
        app_id: &str,
        target_id: &str,
        channel_key: &str,
    ) -> DeployServiceResult<ChannelResponse> {
        self.ensure_release_channel_repo(tenant_id, app_id, target_id, channel_key)
            .await
    }

    async fn retrieve_channel(
        &self,
        tenant_id: i64,
        app_id: &str,
        channel_id: &str,
    ) -> DeployServiceResult<ChannelResponse> {
        self.retrieve_channel_repo(tenant_id, app_id, channel_id)
            .await
    }

    async fn list_channels(
        &self,
        tenant_id: i64,
        app_id: &str,
    ) -> DeployServiceResult<ChannelPage> {
        self.list_channels_repo(tenant_id, app_id).await
    }

    async fn promote_channel(
        &self,
        tenant_id: i64,
        app_id: &str,
        channel_id: &str,
        actor_id: Option<i64>,
        request: &PromoteChannelRequest,
    ) -> DeployServiceResult<ChannelRolloutResponse> {
        self.promote_channel_repo(tenant_id, app_id, channel_id, actor_id, request)
            .await
    }

    async fn list_channel_rollouts(
        &self,
        tenant_id: i64,
        app_id: &str,
        channel_id: &str,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<ChannelRolloutPage> {
        self.list_channel_rollouts_repo(tenant_id, app_id, channel_id, page, page_size)
            .await
    }

    async fn create_app_deployment(
        &self,
        tenant_id: i64,
        actor_id: Option<i64>,
        request: &CreateAppDeploymentRequest,
    ) -> DeployServiceResult<AppDeploymentResponse> {
        self.create_app_deployment_repo(tenant_id, actor_id, request)
            .await
    }

    async fn list_app_deployments(
        &self,
        tenant_id: i64,
        app_id: &str,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<AppDeploymentPage> {
        self.list_app_deployments_repo(tenant_id, app_id, page, page_size)
            .await
    }

    async fn retrieve_app_deployment(
        &self,
        tenant_id: i64,
        app_id: &str,
        deployment_id: &str,
    ) -> DeployServiceResult<AppDeploymentResponse> {
        self.retrieve_app_deployment_repo(tenant_id, app_id, deployment_id)
            .await
    }

    async fn list_review_pending_deployments(
        &self,
        tenant_id: i64,
        limit: i64,
    ) -> DeployServiceResult<Vec<AppDeploymentResponse>> {
        self.list_review_pending_deployments_repo(tenant_id, limit)
            .await
    }

    async fn update_app_deployment_state(
        &self,
        tenant_id: i64,
        app_id: &str,
        deployment_id: &str,
        deployment_status: DeploymentStatus,
        platform_review_ref: Option<&str>,
    ) -> DeployServiceResult<AppDeploymentResponse> {
        self.update_app_deployment_state_repo(
            tenant_id,
            app_id,
            deployment_id,
            deployment_status,
            platform_review_ref,
        )
        .await
    }

    async fn create_signing_identity(
        &self,
        tenant_id: i64,
        actor_id: Option<i64>,
        request: &CreateSigningIdentityRequest,
    ) -> DeployServiceResult<SigningIdentityResponse> {
        self.create_signing_identity_repo(tenant_id, actor_id, request)
            .await
    }

    async fn list_signing_identities(
        &self,
        tenant_id: i64,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<SigningIdentityPage> {
        self.list_signing_identities_repo(tenant_id, page, page_size)
            .await
    }

    async fn retrieve_signing_identity(
        &self,
        tenant_id: i64,
        identity_id: &str,
    ) -> DeployServiceResult<SigningIdentityResponse> {
        self.retrieve_signing_identity_repo(tenant_id, identity_id)
            .await
    }

    async fn insert_usage_event(
        &self,
        command: &InsertUsageEventCommand,
    ) -> DeployServiceResult<UsageEventResponse> {
        self.insert_usage_event_repo(command).await
    }

    async fn list_usage_events(
        &self,
        tenant_id: i64,
        query: &UsageEventQuery,
    ) -> DeployServiceResult<UsageEventPage> {
        self.list_usage_events_repo(tenant_id, query).await
    }

    async fn insert_usage_events_batch(
        &self,
        events: &[UsageEventIngestItem],
    ) -> DeployServiceResult<UsageIngestResult> {
        self.insert_usage_events_batch_repo(events).await
    }

    async fn create_app_database_profile(
        &self,
        tenant_id: i64,
        actor_id: Option<i64>,
        app_id: &str,
        request: &CreateAppDatabaseProfileRequest,
    ) -> DeployServiceResult<AppDatabaseProfileResponse> {
        self.create_app_database_profile_repo(tenant_id, actor_id, app_id, request)
            .await
    }

    async fn list_app_database_profiles(
        &self,
        tenant_id: i64,
        app_id: &str,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<AppDatabaseProfilePage> {
        self.list_app_database_profiles_repo(tenant_id, app_id, page, page_size)
            .await
    }

    async fn retrieve_app_database_profile(
        &self,
        tenant_id: i64,
        app_id: &str,
        profile_id: &str,
    ) -> DeployServiceResult<AppDatabaseProfileResponse> {
        self.retrieve_app_database_profile_repo(tenant_id, app_id, profile_id)
            .await
    }

    async fn update_app_database_profile(
        &self,
        tenant_id: i64,
        actor_id: Option<i64>,
        app_id: &str,
        profile_id: &str,
        request: &UpdateAppDatabaseProfileRequest,
    ) -> DeployServiceResult<AppDatabaseProfileResponse> {
        self.update_app_database_profile_repo(tenant_id, actor_id, app_id, profile_id, request)
            .await
    }

    async fn create_app_database_migration(
        &self,
        tenant_id: i64,
        actor_id: Option<i64>,
        app_id: &str,
        profile_id: &str,
        request: &CreateAppDatabaseMigrationRequest,
    ) -> DeployServiceResult<AppDatabaseMigrationResponse> {
        self.create_app_database_migration_repo(tenant_id, actor_id, app_id, profile_id, request)
            .await
    }

    async fn list_app_database_migrations(
        &self,
        tenant_id: i64,
        app_id: &str,
        profile_id: &str,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<AppDatabaseMigrationPage> {
        self.list_app_database_migrations_repo(tenant_id, app_id, profile_id, page, page_size)
            .await
    }

    async fn retrieve_app_database_migration(
        &self,
        tenant_id: i64,
        app_id: &str,
        profile_id: &str,
        migration_id: &str,
    ) -> DeployServiceResult<AppDatabaseMigrationResponse> {
        self.retrieve_app_database_migration_repo(tenant_id, app_id, profile_id, migration_id)
            .await
    }

    async fn entitlement_usage(&self, tenant_id: i64, dimension: &str) -> DeployServiceResult<i64> {
        self.entitlement_usage_repo(tenant_id, dimension).await
    }

    async fn list_entitlement_projections(
        &self,
        tenant_id: Option<i64>,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<EntitlementProjectionPage> {
        self.list_entitlement_projections_repo(tenant_id, page, page_size)
            .await
    }

    async fn list_build_queue(
        &self,
        tenant_id: Option<i64>,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<BuildQueuePage> {
        self.list_build_queue_repo(tenant_id, page, page_size).await
    }

    async fn list_runner_health(
        &self,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<RunnerHealthPage> {
        self.list_runner_health_repo(page, page_size).await
    }

    async fn create_acme_account(
        &self,
        tenant_id: i64,
        request: &CreateAcmeAccountRequest,
    ) -> DeployServiceResult<AcmeAccountResponse> {
        self.create_acme_account_repo(tenant_id, request).await
    }

    async fn list_acme_accounts(
        &self,
        tenant_id: i64,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<AcmeAccountPage> {
        self.list_acme_accounts_repo(tenant_id, page, page_size)
            .await
    }

    async fn request_certificate_order(
        &self,
        tenant_id: i64,
        request: &RequestCertificateOrderRequest,
    ) -> DeployServiceResult<CertificateOrderResponse> {
        self.request_certificate_order_repo(
            tenant_id,
            &request.certificate_id,
            &request.idempotency_key,
            request.challenge_type.as_deref(),
        )
        .await
    }

    async fn certificate_order_caa_subject(
        &self,
        tenant_id: i64,
        certificate_id: &str,
    ) -> DeployServiceResult<CertificateOrderCaaSubject> {
        self.certificate_order_caa_subject_repo(tenant_id, certificate_id)
            .await
    }

    async fn record_certificate_order_caa_decision(
        &self,
        tenant_id: i64,
        order_id: &str,
        decision: &str,
        checked_at: &str,
    ) -> DeployServiceResult<()> {
        self.record_certificate_order_caa_decision_repo(tenant_id, order_id, decision, checked_at)
            .await
    }

    async fn advance_certificate_order(
        &self,
        tenant_id: i64,
        order_id: &str,
        from_status: &str,
        to_status: &str,
    ) -> DeployServiceResult<String> {
        self.advance_certificate_order_repo(tenant_id, order_id, from_status, to_status)
            .await
    }

    async fn fail_certificate_order(
        &self,
        tenant_id: i64,
        order_id: &str,
        error_code: &str,
    ) -> DeployServiceResult<()> {
        self.fail_certificate_order_repo(tenant_id, order_id, error_code)
            .await
    }

    async fn record_challenge_result(
        &self,
        tenant_id: i64,
        order_id: &str,
        challenge_id: Option<&str>,
        valid: bool,
        error_code: Option<&str>,
    ) -> DeployServiceResult<()> {
        self.record_challenge_result_repo(tenant_id, order_id, challenge_id, valid, error_code)
            .await
    }

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
    ) -> DeployServiceResult<CertificateOrderResponse> {
        self.store_certificate_version_repo(
            tenant_id,
            certificate_version_uuid,
            order_id,
            version_no,
            serial_sha256,
            fingerprint_sha256,
            spki_sha256,
            chain_sha256,
            issuer,
            subject,
            key_algorithm,
            not_before,
            not_after,
            secret_bundle_ref,
            material,
        )
        .await
    }

    async fn retrieve_certificate_material(
        &self,
        tenant_id: i64,
        certificate_version_uuid: &str,
    ) -> DeployServiceResult<Vec<SealedCertificateFile>> {
        self.retrieve_certificate_material_repo(tenant_id, certificate_version_uuid)
            .await
    }

    async fn retrieve_certificate_order(
        &self,
        tenant_id: i64,
        order_id: &str,
    ) -> DeployServiceResult<CertificateOrderResponse> {
        self.retrieve_certificate_order_repo(tenant_id, order_id)
            .await
    }

    async fn list_certificate_orders(
        &self,
        tenant_id: i64,
        certificate_id: &str,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<CertificateOrderPage> {
        self.list_certificate_orders_repo(tenant_id, certificate_id, page, page_size)
            .await
    }

    async fn list_certificate_challenges(
        &self,
        tenant_id: i64,
        order_id: &str,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<CertificateChallengePage> {
        self.list_certificate_challenges_repo(tenant_id, order_id, page, page_size)
            .await
    }

    async fn claim_due_certificate_renewals(
        &self,
        worker_id: &str,
        batch_size: i64,
        lease_seconds: i64,
        now: &str,
    ) -> DeployServiceResult<Vec<CertificateRenewalClaim>> {
        self.claim_due_certificate_renewals_repo(worker_id, batch_size, lease_seconds, now)
            .await
    }

    async fn claim_certificate_orders(
        &self,
        worker_id: &str,
        batch_size: i64,
        lease_seconds: i64,
        now: &str,
    ) -> DeployServiceResult<Vec<CertificateOrderClaim>> {
        self.claim_certificate_orders_repo(worker_id, batch_size, lease_seconds, now)
            .await
    }

    async fn advance_leased_certificate_order(
        &self,
        tenant_id: i64,
        order_id: &str,
        lease_owner: &str,
        from_status: &str,
        to_status: &str,
    ) -> DeployServiceResult<String> {
        self.advance_leased_certificate_order_repo(
            tenant_id,
            order_id,
            lease_owner,
            from_status,
            to_status,
        )
        .await
    }

    async fn fail_expired_certificate_orders(
        &self,
        now: &str,
        batch_size: i64,
    ) -> DeployServiceResult<i64> {
        self.fail_expired_certificate_orders_repo(now, batch_size)
            .await
    }

    async fn mark_certificate_renewal_ordered(
        &self,
        tenant_id: i64,
        renewal_uuid: &str,
        order_uuid: &str,
    ) -> DeployServiceResult<()> {
        self.mark_certificate_renewal_ordered_repo(tenant_id, renewal_uuid, order_uuid)
            .await
    }

    async fn fail_certificate_renewal(
        &self,
        tenant_id: i64,
        renewal_uuid: &str,
        error_code: &str,
    ) -> DeployServiceResult<()> {
        self.fail_certificate_renewal_repo(tenant_id, renewal_uuid, error_code)
            .await
    }

    async fn sweep_expired_certificates(
        &self,
        batch_size: i64,
        now: &str,
    ) -> DeployServiceResult<ExpiredCertificateSweep> {
        self.sweep_expired_certificates_repo(batch_size, now).await
    }

    async fn list_certificate_renewals(
        &self,
        tenant_id: i64,
        certificate_id: &str,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<CertificateRenewalPage> {
        self.list_certificate_renewals_repo(tenant_id, certificate_id, page, page_size)
            .await
    }

    async fn run_retention(
        &self,
        dry_run: bool,
        package_retention_days: i64,
        release_retention_days: i64,
        build_log_retention_days: i64,
    ) -> DeployServiceResult<RetentionRunResponse> {
        self.run_retention_repo(
            dry_run,
            package_retention_days,
            release_retention_days,
            build_log_retention_days,
        )
        .await
    }

    async fn rebuild_usage_daily(
        &self,
        window_start: Option<&str>,
        window_end: Option<&str>,
    ) -> DeployServiceResult<UsageReconciliationResponse> {
        self.rebuild_usage_daily_repo(window_start, window_end)
            .await
    }

    async fn list_signing_identity_health(
        &self,
        tenant_id: Option<i64>,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<SigningIdentityHealthPage> {
        self.list_signing_identity_health_repo(tenant_id, page, page_size)
            .await
    }

    async fn create_app_environment(
        &self,
        tenant_id: i64,
        actor_id: Option<i64>,
        app_id: &str,
        request: &CreateAppEnvironmentRequest,
    ) -> DeployServiceResult<AppEnvironmentResponse> {
        self.create_app_environment_repo(tenant_id, actor_id, app_id, request)
            .await
    }

    async fn list_app_environments(
        &self,
        tenant_id: i64,
        app_id: &str,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<AppEnvironmentPage> {
        self.list_app_environments_repo(tenant_id, app_id, page, page_size)
            .await
    }

    async fn retrieve_app_environment(
        &self,
        tenant_id: i64,
        app_id: &str,
        environment_id: &str,
    ) -> DeployServiceResult<AppEnvironmentResponse> {
        self.retrieve_app_environment_repo(tenant_id, app_id, environment_id)
            .await
    }

    async fn update_app_environment(
        &self,
        tenant_id: i64,
        actor_id: Option<i64>,
        app_id: &str,
        environment_id: &str,
        request: &UpdateAppEnvironmentRequest,
    ) -> DeployServiceResult<AppEnvironmentResponse> {
        self.update_app_environment_repo(tenant_id, actor_id, app_id, environment_id, request)
            .await
    }

    async fn promote_environment(
        &self,
        tenant_id: i64,
        actor_id: Option<i64>,
        app_id: &str,
        environment_id: &str,
        request: &PromoteEnvironmentRequest,
    ) -> DeployServiceResult<EnvironmentPromotionResponse> {
        self.promote_environment_repo(tenant_id, actor_id, app_id, environment_id, request)
            .await
    }

    async fn list_environment_promotions(
        &self,
        tenant_id: i64,
        app_id: &str,
        environment_id: &str,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<EnvironmentPromotionPage> {
        self.list_environment_promotions_repo(tenant_id, app_id, environment_id, page, page_size)
            .await
    }

    async fn match_repository_by_url(
        &self,
        clone_url: &str,
    ) -> DeployServiceResult<Option<RepositoryMatch>> {
        let matched = self.match_repository_by_url_repo(clone_url).await?;
        Ok(matched.map(|matched| RepositoryMatch {
            tenant_id: matched.tenant_id,
            app_id: matched.app_id,
            repository_id: matched.repository_id,
            repository_internal_id: matched.repository_internal_id,
            app_internal_id: matched.app_internal_id,
            default_branch: matched.default_branch,
        }))
    }

    async fn list_trigger_targets(&self, app_id: &str) -> DeployServiceResult<Vec<TriggerTarget>> {
        self.list_trigger_targets_repo(app_id).await
    }

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
    ) -> DeployServiceResult<(SourceEventResponse, bool)> {
        self.ingest_source_event_repo(
            &crate::source_events::MatchedRepository {
                tenant_id: matched.tenant_id,
                app_id: matched.app_id.clone(),
                repository_id: matched.repository_id.clone(),
                repository_internal_id: matched.repository_internal_id,
                app_internal_id: matched.app_internal_id,
                default_branch: matched.default_branch.clone(),
            },
            event_kind,
            source_ref,
            source_commit,
            commit_message,
            sender_ref,
            payload_sha256,
        )
        .await
    }

    async fn update_source_event_result(
        &self,
        tenant_id: i64,
        event_id: &str,
        processed: bool,
        builds_triggered: i32,
        error_code: Option<&str>,
    ) -> DeployServiceResult<()> {
        self.update_source_event_result_repo(
            tenant_id,
            event_id,
            processed,
            builds_triggered,
            error_code,
        )
        .await
    }

    async fn list_source_events(
        &self,
        tenant_id: Option<i64>,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<SourceEventPage> {
        self.list_source_events_repo(tenant_id, page, page_size)
            .await
    }

    async fn app_domain_suffix_override(
        &self,
        tenant_id: i64,
        app_id: &str,
    ) -> DeployServiceResult<Option<Vec<String>>> {
        let app_id = crate::support::resolve_app_internal_id(&self.pool, tenant_id, app_id).await?;
        let config = self.app_domain_config_repo(app_id).await?;
        Ok(config.override_suffixes)
    }

    async fn ensure_platform_app_zones(
        &self,
        tenant_id: i64,
        organization_id: i64,
        actor_id: Option<i64>,
        suffixes: &[String],
    ) -> DeployServiceResult<usize> {
        self.ensure_platform_app_zones_repo(tenant_id, organization_id, actor_id, suffixes)
            .await
    }

    async fn provision_app_default_domains(
        &self,
        tenant_id: i64,
        organization_id: i64,
        actor_id: Option<i64>,
        app_id: &str,
        environment: &str,
    ) -> DeployServiceResult<ProvisionAppDomainsResult> {
        self.provision_app_default_domains_repo(
            tenant_id,
            organization_id,
            actor_id,
            app_id,
            environment,
        )
        .await
    }

    async fn resolve_server_by_hostname(
        &self,
        hostname: &str,
        environment: &str,
    ) -> DeployServiceResult<Option<ResolvedDeployServer>> {
        self.resolve_active_app_by_hostname_repo(hostname, environment)
            .await
    }
}

#[async_trait]
impl DeployRuntimeAssignmentRepositoryPort for DeployRepository {
    async fn latest_runtime_assignment(
        &self,
        target_uuid: &str,
    ) -> sdkwork_deploy_contract::DeployServiceResult<Option<RuntimeAssignmentState>> {
        self.latest_runtime_assignment_repo(target_uuid).await
    }

    async fn begin_runtime_assignment_mutation(
        &self,
        target_uuid: &str,
        tenant_id: i64,
    ) -> sdkwork_deploy_contract::DeployServiceResult<Box<dyn DeployRuntimeAssignmentMutationPort>>
    {
        self.begin_runtime_assignment_mutation_repo(target_uuid, tenant_id)
            .await
    }

    async fn claim_due_runtime_assignments(
        &self,
        maximum_items: i64,
        now: &str,
        lease_owner: &str,
        lease_expires_at: &str,
        maximum_attempts: i32,
    ) -> sdkwork_deploy_contract::DeployServiceResult<Vec<RuntimeAssignmentState>> {
        self.claim_due_runtime_assignments_repo(
            maximum_items,
            now,
            lease_owner,
            lease_expires_at,
            maximum_attempts,
        )
        .await
    }

    async fn mark_runtime_assignment_published(
        &self,
        assignment_uuid: &str,
        lease_owner: &str,
        receipt: &RuntimeAssignmentReceipt,
        published_at: &str,
    ) -> sdkwork_deploy_contract::DeployServiceResult<()> {
        self.mark_runtime_assignment_published_repo(
            assignment_uuid,
            lease_owner,
            receipt,
            published_at,
        )
        .await
    }

    async fn mark_runtime_assignment_failed(
        &self,
        assignment_uuid: &str,
        lease_owner: &str,
        error_code: &str,
        next_attempt_at: Option<&str>,
        updated_at: &str,
    ) -> sdkwork_deploy_contract::DeployServiceResult<()> {
        self.mark_runtime_assignment_failed_repo(
            assignment_uuid,
            lease_owner,
            error_code,
            next_attempt_at,
            updated_at,
        )
        .await
    }

    async fn list_runtime_assignments_requiring_observation(
        &self,
        maximum_items: i64,
    ) -> sdkwork_deploy_contract::DeployServiceResult<Vec<RuntimeAssignmentState>> {
        self.list_runtime_assignments_requiring_observation_repo(maximum_items)
            .await
    }

    async fn list_active_runtime_assignments_after(
        &self,
        after_target_uuid: Option<&str>,
        maximum_items: i64,
    ) -> sdkwork_deploy_contract::DeployServiceResult<Vec<RuntimeAssignmentState>> {
        self.list_active_runtime_assignments_after_repo(after_target_uuid, maximum_items)
            .await
    }

    async fn persist_runtime_observation(
        &self,
        assignment_uuid: &str,
        observation: &RuntimeObservationEvidence,
        ingested_at: &str,
    ) -> sdkwork_deploy_contract::DeployServiceResult<RuntimeObservationPersistenceResult> {
        self.persist_runtime_observation_repo(assignment_uuid, observation, ingested_at)
            .await
    }
}
