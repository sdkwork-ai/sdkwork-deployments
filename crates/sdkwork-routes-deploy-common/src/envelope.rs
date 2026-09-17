//! Map deploy domain DTOs to SdkWork HTTP API v3 envelope payloads.

use sdkwork_deploy_contract::{
    AcmeAccountPage, AcmeAccountResponse, AppDatabaseMigrationPage, AppDatabaseMigrationResponse,
    AppDatabaseProfilePage, AppDatabaseProfileResponse, AppDeploymentPage, AppDeploymentResponse,
    AppEnvironmentPage, AppEnvironmentResponse, AppPage, AppReleasePage, AppReleaseResponse,
    AppResponse, ArtifactPage, ArtifactResponse, AuditLogPage, AuditLogResponse, BuildPage,
    BuildQueueItemResponse, BuildQueuePage, BuildResponse, BuildTemplatePage,
    BuildTemplateResponse, CertificateChallengePage, CertificateChallengeResponse,
    CertificateOrderPage, CertificateOrderResponse, CertificatePage, CertificateRenewalPage,
    CertificateRenewalResponse, CertificateResponse, ChannelPage, ChannelResponse,
    ChannelRolloutPage, ChannelRolloutResponse, CloudAccountPage, CloudAccountResponse,
    DomainHostnamePage, DomainHostnameResponse, DomainVerifyResponse, DomainZonePage,
    DomainZoneResponse, EntitlementProjectionPage, EntitlementProjectionResponse, EnvVariablePage,
    EnvVariableResponse, EnvironmentPromotionPage, EnvironmentPromotionResponse, HealthCheckPage,
    HealthCheckResponse, NginxConfigPage, NginxConfigResponse, NginxReloadResponse,
    NginxStatusResponse, NginxValidateResponse, NodeClusterPage, NodeClusterResponse, PackagePage,
    PackageResponse, PlatformTargetPage, PlatformTargetResponse, RunnerHealthPage,
    RunnerHealthResponse, ServerPage, ServerResponse, SigningIdentityHealthPage,
    SigningIdentityHealthResponse, SigningIdentityPage, SigningIdentityResponse, SourceEventPage,
    SourceEventResponse, SourceRepositoryPage, SourceRepositoryResponse, UsageEventPage,
    UsageEventResponse,
};
use sdkwork_deploy_core::normalize_pagination;
use sdkwork_utils_rust::{PageInfo, PageMode, SdkWorkPageData, SdkWorkResourceData};

pub fn resource<T>(item: T) -> SdkWorkResourceData<T> {
    SdkWorkResourceData { item }
}

pub fn app_page(page: AppPage) -> SdkWorkPageData<AppResponse> {
    offset_page(page.items, page.page, page.page_size, page.total)
}

pub fn platform_target_page(page: PlatformTargetPage) -> SdkWorkPageData<PlatformTargetResponse> {
    offset_page(page.items, page.page, page.page_size, page.total)
}

pub fn source_repository_page(
    page: SourceRepositoryPage,
) -> SdkWorkPageData<SourceRepositoryResponse> {
    offset_page(page.items, page.page, page.page_size, page.total)
}

pub fn build_template_page(page: BuildTemplatePage) -> SdkWorkPageData<BuildTemplateResponse> {
    offset_page(page.items, page.page, page.page_size, page.total)
}

pub fn build_page(page: BuildPage) -> SdkWorkPageData<BuildResponse> {
    offset_page(page.items, page.page, page.page_size, page.total)
}

pub fn package_page(page: PackagePage) -> SdkWorkPageData<PackageResponse> {
    offset_page(page.items, page.page, page.page_size, page.total)
}

pub fn app_release_page(page: AppReleasePage) -> SdkWorkPageData<AppReleaseResponse> {
    offset_page(page.items, page.page, page.page_size, page.total)
}

pub fn channel_page(page: ChannelPage) -> SdkWorkPageData<ChannelResponse> {
    offset_page(page.items, page.page, page.page_size, page.total)
}

pub fn channel_rollout_page(page: ChannelRolloutPage) -> SdkWorkPageData<ChannelRolloutResponse> {
    offset_page(page.items, page.page, page.page_size, page.total)
}

pub fn app_deployment_page(page: AppDeploymentPage) -> SdkWorkPageData<AppDeploymentResponse> {
    offset_page(page.items, page.page, page.page_size, page.total)
}

pub fn signing_identity_page(
    page: SigningIdentityPage,
) -> SdkWorkPageData<SigningIdentityResponse> {
    offset_page(page.items, page.page, page.page_size, page.total)
}

pub fn usage_event_page(page: UsageEventPage) -> SdkWorkPageData<UsageEventResponse> {
    offset_page(page.items, page.page, page.page_size, page.total)
}

pub fn app_database_profile_page(
    page: AppDatabaseProfilePage,
) -> SdkWorkPageData<AppDatabaseProfileResponse> {
    offset_page(page.items, page.page, page.page_size, page.total)
}

pub fn app_database_migration_page(
    page: AppDatabaseMigrationPage,
) -> SdkWorkPageData<AppDatabaseMigrationResponse> {
    offset_page(page.items, page.page, page.page_size, page.total)
}

pub fn entitlement_projection_page(
    page: EntitlementProjectionPage,
) -> SdkWorkPageData<EntitlementProjectionResponse> {
    offset_page(page.items, page.page, page.page_size, page.total)
}

pub fn build_queue_page(page: BuildQueuePage) -> SdkWorkPageData<BuildQueueItemResponse> {
    offset_page(page.items, page.page, page.page_size, page.total)
}

pub fn runner_health_page(page: RunnerHealthPage) -> SdkWorkPageData<RunnerHealthResponse> {
    offset_page(page.items, page.page, page.page_size, page.total)
}

pub fn acme_account_page(page: AcmeAccountPage) -> SdkWorkPageData<AcmeAccountResponse> {
    offset_page(page.items, page.page, page.page_size, page.total)
}

pub fn certificate_order_page(
    page: CertificateOrderPage,
) -> SdkWorkPageData<CertificateOrderResponse> {
    offset_page(page.items, page.page, page.page_size, page.total)
}

pub fn certificate_challenge_page(
    page: CertificateChallengePage,
) -> SdkWorkPageData<CertificateChallengeResponse> {
    offset_page(page.items, page.page, page.page_size, page.total)
}

pub fn signing_identity_health_page(
    page: SigningIdentityHealthPage,
) -> SdkWorkPageData<SigningIdentityHealthResponse> {
    offset_page(page.items, page.page, page.page_size, page.total)
}

pub fn app_environment_page(page: AppEnvironmentPage) -> SdkWorkPageData<AppEnvironmentResponse> {
    offset_page(page.items, page.page, page.page_size, page.total)
}

pub fn environment_promotion_page(
    page: EnvironmentPromotionPage,
) -> SdkWorkPageData<EnvironmentPromotionResponse> {
    offset_page(page.items, page.page, page.page_size, page.total)
}

pub fn source_event_page(page: SourceEventPage) -> SdkWorkPageData<SourceEventResponse> {
    offset_page(page.items, page.page, page.page_size, page.total)
}

pub fn domain_zone_page(page: DomainZonePage) -> SdkWorkPageData<DomainZoneResponse> {
    offset_page(page.items, page.page, page.page_size, page.total)
}

pub fn domain_hostname_page(page: DomainHostnamePage) -> SdkWorkPageData<DomainHostnameResponse> {
    offset_page(page.items, page.page, page.page_size, page.total)
}

pub fn env_variable_page(data: EnvVariablePage) -> SdkWorkPageData<EnvVariableResponse> {
    let page_size = data.items.len().max(1) as i32;
    offset_page(data.items, 1, page_size, data.total)
}

pub fn certificate_page(
    page: CertificatePage,
    page_num: i32,
    page_size: i32,
) -> SdkWorkPageData<CertificateResponse> {
    offset_page(page.items, page_num, page_size, page.total)
}

pub fn certificate_renewal_page(
    page: CertificateRenewalPage,
    page_num: i32,
    page_size: i32,
) -> SdkWorkPageData<CertificateRenewalResponse> {
    offset_page(page.items, page_num, page_size, page.total)
}

/// Envelopes a page of cloud accounts.
///
/// The item order is carried through untouched. `CloudAccountPage` documents
/// narrowest-scope-first as part of the contract, and re-sorting here would make
/// the wire order differ from the order the server resolves in — which is the one
/// property a picker relies on to show "the account a create would reuse" first.
pub fn cloud_account_page(
    page: CloudAccountPage,
    page_num: i32,
    page_size: i32,
) -> SdkWorkPageData<CloudAccountResponse> {
    offset_page(page.items, page_num, page_size, page.total)
}

pub fn artifact_page(
    page: ArtifactPage,
    page_num: i32,
    page_size: i32,
) -> SdkWorkPageData<ArtifactResponse> {
    offset_page(page.items, page_num, page_size, page.total)
}

pub fn health_check_page(data: HealthCheckPage) -> SdkWorkPageData<HealthCheckResponse> {
    let page_size = data.items.len().max(1) as i32;
    offset_page(data.items, 1, page_size, data.total)
}

pub fn nginx_config_page(page: NginxConfigPage) -> SdkWorkPageData<NginxConfigResponse> {
    offset_page(page.items, page.page, page.page_size, page.total)
}

pub fn server_page(
    page: ServerPage,
    page_num: i32,
    page_size: i32,
) -> SdkWorkPageData<ServerResponse> {
    offset_page(page.items, page_num, page_size, page.total)
}

pub fn node_cluster_page(
    page: NodeClusterPage,
    page_num: i32,
    page_size: i32,
) -> SdkWorkPageData<NodeClusterResponse> {
    offset_page(page.items, page_num, page_size, page.total)
}

/// Offset page for the audit log surface.
///
/// `nextCursor` is forwarded so that a client which paged with `page` /
/// `page_size` can switch to keyset continuation instead of walking deeper
/// `OFFSET` windows (PAGINATION_SPEC §6); `hasMore` keeps coming from the exact
/// count, which offset mode computes anyway.
pub fn audit_log_page(page: AuditLogPage) -> SdkWorkPageData<AuditLogResponse> {
    let mut data = offset_page(page.items, page.page, page.page_size, page.total);
    data.page_info.next_cursor = page.next_cursor;
    data
}

pub fn domain_verify(item: DomainVerifyResponse) -> SdkWorkResourceData<DomainVerifyResponse> {
    resource(item)
}

pub fn nginx_validate(item: NginxValidateResponse) -> SdkWorkResourceData<NginxValidateResponse> {
    resource(item)
}

pub fn nginx_reload(item: NginxReloadResponse) -> SdkWorkResourceData<NginxReloadResponse> {
    resource(item)
}

pub fn nginx_status(item: NginxStatusResponse) -> SdkWorkResourceData<NginxStatusResponse> {
    resource(item)
}

/// Cursor（keyset）模式 pageInfo（PAGINATION_SPEC §3/§6）：growing 集合
/// （审计日志、部署记录）使用不透明 cursor 延续，避免深 OFFSET 与全表 COUNT。
pub fn cursor_page<T>(
    items: Vec<T>,
    page_size: i32,
    next_cursor: Option<String>,
    has_more: Option<bool>,
) -> SdkWorkPageData<T> {
    SdkWorkPageData {
        items,
        page_info: PageInfo {
            mode: PageMode::Cursor,
            page: None,
            page_size: Some(page_size),
            total_items: None,
            total_pages: None,
            next_cursor,
            has_more,
        },
    }
}

fn offset_page<T>(items: Vec<T>, page: i32, page_size: i32, total: i64) -> SdkWorkPageData<T> {
    let (page, page_size) = normalize_pagination(page, page_size);
    let total_pages = if page_size > 0 {
        Some(((total as f64) / page_size as f64).ceil() as i32)
    } else {
        None
    };
    SdkWorkPageData {
        items,
        page_info: PageInfo {
            mode: PageMode::Offset,
            page: Some(page),
            page_size: Some(page_size),
            total_items: Some(total.to_string()),
            total_pages,
            next_cursor: None,
            // PAGINATION_SPEC §8: UI 依赖 hasMore 渲染"加载更多"。
            has_more: Some(total > (page as i64) * page_size as i64),
        },
    }
}
