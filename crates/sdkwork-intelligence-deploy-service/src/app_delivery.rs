//! Unified application delivery service surface (REQ-2026-0002): validation
//! of app-kind/platform rules, semantic versions, package formats and size
//! ceilings, then delegation to the repository port.

use sdkwork_deploy_contract::{
    AppDatabaseMigrationPage, AppDatabaseMigrationResponse, AppDatabaseProfilePage,
    AppDatabaseProfileResponse, AppEnvironmentPage, AppEnvironmentResponse, AppKind, AppPage,
    AppReleasePage, AppReleaseResponse, AppResponse, BuildPage, BuildResponse, BuildTemplatePage,
    BuildTemplateResponse, ChannelKey, ChannelPage, ChannelResponse, ChannelRolloutPage,
    ChannelRolloutResponse, CreateAppDatabaseMigrationRequest, CreateAppDatabaseProfileRequest,
    CreateAppDeploymentRequest, CreateAppEnvironmentRequest, CreateAppReleaseRequest,
    CreateAppRequest, CreateBuildRequest, CreateBuildTemplateRequest, CreatePlatformTargetRequest,
    CreateSigningIdentityRequest, CreateSourceRepositoryRequest, DeployAppRequestContext,
    DeployServiceError, DeployServiceResult, DeploymentStatus, EnvironmentPromotionPage,
    EnvironmentPromotionResponse, PackagePage, PackageResponse, PlatformTargetPage,
    PlatformTargetResponse, PromoteChannelRequest, PromoteEnvironmentRequest,
    RegisterPackageRequest, ReleaseStatus, SigningIdentityPage, SigningIdentityResponse,
    SourceRepositoryPage, SourceRepositoryResponse, UpdateAppDatabaseProfileRequest,
    UpdateAppEnvironmentRequest, UpdateAppRequest, UpdateBuildStateRequest, UsageEventPage,
    UsageEventQuery, ENTITLEMENT_DIMENSION_ACTIVE_APPS, ENTITLEMENT_DIMENSION_BUILD_CONCURRENCY,
    ENTITLEMENT_DIMENSION_DEPLOYMENT_COUNT, ENTITLEMENT_DIMENSION_PACKAGE_STORAGE_BYTES,
    ENTITLEMENT_DIMENSION_PLATFORM_TARGETS, ENTITLEMENT_DIMENSION_RELEASE_COUNT,
    USAGE_DIMENSION_BUILD_MINUTES, USAGE_DIMENSION_DEPLOYMENT_COUNT,
    USAGE_DIMENSION_PACKAGE_STORAGE_BYTES,
};
use sdkwork_deploy_core::{
    required_identity_field, validate_app_kind_platform, validate_catalog_name,
    validate_database_engine, validate_migration_name, validate_migration_strategy,
    validate_migration_version, validate_package_format_for_platform, validate_package_size,
    validate_platform_identity, validate_profile_key, validate_profile_status, validate_sha256_hex,
    RequiredIdentityField, SemanticVersion,
};

use crate::repository::{InsertAuditLogCommand, InsertUsageEventCommand};
use crate::DeployService;

const CHANNEL_KEYS: &[&str] = &["stable", "beta", "alpha", "qa"];
const REPO_PROVIDERS: &[&str] = &["GITHUB", "GITEE", "GITLAB", "SELF_HOSTED"];
const CLONE_MODES: &[&str] = &["FULL", "SHALLOW"];

impl DeployService {
    fn tenant_id(context: &DeployAppRequestContext) -> DeployServiceResult<i64> {
        if context.tenant_id <= 0 {
            return Err(DeployServiceError::forbidden(
                "app delivery operations require tenant authorization",
            ));
        }
        Ok(context.tenant_id)
    }

    pub(crate) async fn audit_app_action(
        &self,
        context: &DeployAppRequestContext,
        action: &str,
        target_uuid: &str,
    ) -> DeployServiceResult<()> {
        self.repository
            .insert_audit_log(InsertAuditLogCommand {
                tenant_id: context.tenant_id,
                organization_id: context.organization_id.unwrap_or(0),
                operator_id: context.actor_id.unwrap_or(0),
                action: action.to_owned(),
                target_type: "app".to_owned(),
                target_id: None,
                target_uuid: Some(target_uuid.to_owned()),
            })
            .await
    }

    /// Emits a usage fact without blocking the primary operation: metering
    /// failures are logged and swallowed (TECH §4.6 fire-and-warn semantics).
    /// The deduplication key makes retried flows idempotent per fact.
    async fn record_usage(&self, command: InsertUsageEventCommand) {
        if let Err(error) = self.repository.insert_usage_event(&command).await {
            tracing::warn!(
                "usage metering skipped tenant={} dimension={}: {error}",
                command.tenant_id,
                command.dimension
            );
        }
    }

    /// UTC month-start used as the metering period boundary; falls back to
    /// the current month when the source timestamp is missing or malformed.
    fn billing_period_start(at: Option<&str>) -> String {
        use chrono::Datelike;
        let instant = at
            .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
            .map(|value| value.with_timezone(&chrono::Utc))
            .unwrap_or_else(chrono::Utc::now);
        chrono::NaiveDate::from_ymd_opt(instant.year(), instant.month(), 1)
            .and_then(|date| date.and_hms_opt(0, 0, 0))
            .map(|naive| chrono::DateTime::from_naive_utc_and_offset(naive, chrono::Utc))
            .unwrap_or(instant)
            .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
    }

    // -- apps -----------------------------------------------------------------

    pub async fn create_app(
        &self,
        context: &DeployAppRequestContext,
        request: &CreateAppRequest,
    ) -> DeployServiceResult<AppResponse> {
        let tenant_id = Self::tenant_id(context)?;
        self.enforce_entitlement(tenant_id, ENTITLEMENT_DIMENSION_ACTIVE_APPS)
            .await?;
        if request.name.trim().is_empty() {
            return Err(DeployServiceError::validation("app name is required"));
        }
        if let Some(slug) = request.slug.as_deref() {
            if !is_bounded_slug(slug) {
                return Err(DeployServiceError::validation(
                    "app slug must be 1..=120 lowercase ascii letters, digits, and hyphens",
                ));
            }
        }
        let app = self
            .repository
            .create_app(
                tenant_id,
                context.organization_id,
                context.actor_id,
                request,
            )
            .await?;
        self.audit_app_action(context, "app.create", &app.id)
            .await?;
        // Every app is a publishable surface: reconcile the app's default
        // publishing domains (`<appDomainLabel>.app[-<env>].<suffix>`) for
        // **all** lifecycle environments so `*.app-dev.*`, `*.app-test.*`,
        // `*.app-staging.*` and `*.app-demo.*` resolve from the moment the app
        // exists — not only the app's `defaultEnvironment`.
        self.provision_app_default_domains_all_environments(context, &app.id)
            .await?;
        Ok(app)
    }

    pub async fn list_apps(
        &self,
        context: &DeployAppRequestContext,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<AppPage> {
        let tenant_id = Self::tenant_id(context)?;
        self.repository.list_apps(tenant_id, page, page_size).await
    }

    pub async fn retrieve_app(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
    ) -> DeployServiceResult<AppResponse> {
        let tenant_id = Self::tenant_id(context)?;
        self.repository.retrieve_app(tenant_id, app_id).await
    }

    pub async fn update_app(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        request: &UpdateAppRequest,
    ) -> DeployServiceResult<AppResponse> {
        let tenant_id = Self::tenant_id(context)?;
        if let Some(name) = request.name.as_deref() {
            if name.trim().is_empty() {
                return Err(DeployServiceError::validation("app name must not be empty"));
            }
        }
        let app = self
            .repository
            .update_app(tenant_id, context.actor_id, app_id, request)
            .await?;
        self.audit_app_action(context, "app.update", &app.id)
            .await?;
        Ok(app)
    }

    // -- platform targets ------------------------------------------------------

    pub async fn create_platform_target(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        request: &CreatePlatformTargetRequest,
    ) -> DeployServiceResult<PlatformTargetResponse> {
        let tenant_id = Self::tenant_id(context)?;
        self.enforce_entitlement(tenant_id, ENTITLEMENT_DIMENSION_PLATFORM_TARGETS)
            .await?;
        if request.target_key.trim().is_empty() || request.target_key.len() > 120 {
            return Err(DeployServiceError::validation(
                "targetKey must be 1..=120 characters",
            ));
        }
        if let Some(channels) = request.allowed_channels.as_deref() {
            validate_channel_keys(channels)?;
        }
        // app-kind/platform compatibility requires the app's kind.
        let app = self.repository.retrieve_app(tenant_id, app_id).await?;
        let Some(app_kind) = AppKind::parse(&app.app_kind) else {
            return Err(DeployServiceError::validation(format!(
                "app kind {} is unknown",
                app.app_kind
            )));
        };
        validate_app_kind_platform(app_kind.as_str(), request.platform.as_str())
            .map_err(DeployServiceError::validation)?;

        // Platform identity is mandatory for platforms that require one.
        let identity = match request.platform.as_str() {
            "IOS" => request.bundle_id.as_deref(),
            "ANDROID" => request.package_name.as_deref(),
            "WECHAT" | "DOUYIN" => request.app_id.as_deref(),
            "HARMONYOS" => request.bundle_name.as_deref(),
            _ => None,
        };
        if required_identity_field(request.platform.as_str()) != RequiredIdentityField::None {
            let identity = identity.ok_or_else(|| {
                DeployServiceError::validation(format!(
                    "platform {} requires {}",
                    request.platform.as_str(),
                    required_identity_field(request.platform.as_str()).as_str()
                ))
            })?;
            validate_platform_identity(request.platform.as_str(), identity)
                .map_err(DeployServiceError::validation)?;
        }

        let target = self
            .repository
            .create_platform_target(tenant_id, app_id, context.actor_id, request)
            .await?;
        self.audit_app_action(context, "platformTarget.create", &target.id)
            .await?;
        Ok(target)
    }

    pub async fn list_platform_targets(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
    ) -> DeployServiceResult<PlatformTargetPage> {
        let tenant_id = Self::tenant_id(context)?;
        self.repository
            .list_platform_targets(tenant_id, app_id)
            .await
    }

    pub async fn retrieve_platform_target(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        target_id: &str,
    ) -> DeployServiceResult<PlatformTargetResponse> {
        let tenant_id = Self::tenant_id(context)?;
        self.repository
            .retrieve_platform_target(tenant_id, app_id, target_id)
            .await
    }

    // -- source repositories ----------------------------------------------------

    pub async fn create_source_repository(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        request: &CreateSourceRepositoryRequest,
    ) -> DeployServiceResult<SourceRepositoryResponse> {
        let tenant_id = Self::tenant_id(context)?;
        if request.repo_key.trim().is_empty() || request.repo_key.len() > 120 {
            return Err(DeployServiceError::validation(
                "repoKey must be 1..=120 characters",
            ));
        }
        if !REPO_PROVIDERS.contains(&request.repo_provider.as_str()) {
            return Err(DeployServiceError::validation(format!(
                "repoProvider must be one of {}",
                REPO_PROVIDERS.join(", ")
            )));
        }
        if !CLONE_MODES
            .iter()
            .any(|mode| Some(*mode) == request.clone_mode.as_deref())
        {
            return Err(DeployServiceError::validation(
                "cloneMode must be FULL or SHALLOW",
            ));
        }
        validate_repo_url(&request.repo_url)?;

        let repo = self
            .repository
            .create_source_repository(tenant_id, app_id, context.actor_id, request)
            .await?;
        self.audit_app_action(context, "sourceRepository.create", &repo.id)
            .await?;
        Ok(repo)
    }

    pub async fn list_source_repositories(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
    ) -> DeployServiceResult<SourceRepositoryPage> {
        let tenant_id = Self::tenant_id(context)?;
        self.repository
            .list_source_repositories(tenant_id, app_id)
            .await
    }

    pub async fn retrieve_source_repository(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        repo_id: &str,
    ) -> DeployServiceResult<SourceRepositoryResponse> {
        let tenant_id = Self::tenant_id(context)?;
        self.repository
            .retrieve_source_repository(tenant_id, app_id, repo_id)
            .await
    }

    // -- build templates ---------------------------------------------------------

    pub async fn create_build_template(
        &self,
        context: &DeployAppRequestContext,
        request: &CreateBuildTemplateRequest,
    ) -> DeployServiceResult<BuildTemplateResponse> {
        let tenant_id = Self::tenant_id(context)?;
        if request.template_name.trim().is_empty() || request.template_name.len() > 200 {
            return Err(DeployServiceError::validation(
                "templateName must be 1..=200 characters",
            ));
        }
        if request.template_version.trim().is_empty() || request.template_version.len() > 64 {
            return Err(DeployServiceError::validation(
                "templateVersion must be 1..=64 characters",
            ));
        }
        if let Some(commands) = request.commands.as_deref() {
            if commands.len() > 64 {
                return Err(DeployServiceError::validation(
                    "commands must contain at most 64 entries",
                ));
            }
            for command in commands {
                if command.trim().is_empty() || command.len() > 500 {
                    return Err(DeployServiceError::validation(
                        "each command must be 1..=500 characters",
                    ));
                }
            }
        }
        let template = self
            .repository
            .create_build_template(tenant_id, context.actor_id, request)
            .await?;
        self.audit_app_action(context, "buildTemplate.create", &template.id)
            .await?;
        Ok(template)
    }

    pub async fn list_build_templates(
        &self,
        context: &DeployAppRequestContext,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<BuildTemplatePage> {
        let tenant_id = Self::tenant_id(context)?;
        self.repository
            .list_build_templates(tenant_id, page, page_size)
            .await
    }

    pub async fn retrieve_build_template(
        &self,
        context: &DeployAppRequestContext,
        template_id: &str,
    ) -> DeployServiceResult<BuildTemplateResponse> {
        let tenant_id = Self::tenant_id(context)?;
        self.repository
            .retrieve_build_template(tenant_id, template_id)
            .await
    }

    // -- builds --------------------------------------------------------------------

    pub async fn create_build(
        &self,
        context: &DeployAppRequestContext,
        request: &CreateBuildRequest,
    ) -> DeployServiceResult<BuildResponse> {
        let tenant_id = Self::tenant_id(context)?;
        self.enforce_entitlement(tenant_id, ENTITLEMENT_DIMENSION_BUILD_CONCURRENCY)
            .await?;
        if request.idempotency_key.trim().is_empty() {
            return Err(DeployServiceError::validation("idempotencyKey is required"));
        }
        if let Some(version) = request.semantic_version.as_deref() {
            SemanticVersion::parse(version).map_err(|error| {
                DeployServiceError::validation(format!("semanticVersion: {error}"))
            })?;
        }
        let build = self
            .repository
            .create_build(tenant_id, context.actor_id, request)
            .await?;
        self.audit_app_action(context, "build.create", &build.id)
            .await?;
        Ok(build)
    }

    pub async fn list_builds(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<BuildPage> {
        let tenant_id = Self::tenant_id(context)?;
        self.repository
            .list_builds(tenant_id, app_id, page, page_size)
            .await
    }

    pub async fn retrieve_build(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        build_id: &str,
    ) -> DeployServiceResult<BuildResponse> {
        let tenant_id = Self::tenant_id(context)?;
        self.repository
            .retrieve_build(tenant_id, app_id, build_id)
            .await
    }

    /// Runner-reported state transition (typed executor contract).
    pub async fn update_build_state(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        build_id: &str,
        request: &UpdateBuildStateRequest,
    ) -> DeployServiceResult<BuildResponse> {
        let tenant_id = Self::tenant_id(context)?;
        if request.runner_node_uuid.trim().is_empty() {
            return Err(DeployServiceError::validation("runnerNodeUuid is required"));
        }
        let build = self
            .repository
            .update_build_state(tenant_id, app_id, build_id, request)
            .await?;
        self.audit_app_action(context, "build.stateUpdate", &build.id)
            .await?;
        if matches!(
            build.build_status.as_str(),
            "SUCCEEDED" | "FAILED" | "CANCELLED" | "TIMED_OUT"
        ) {
            // Terminal builds bill wall-clock compute minutes, floored at one
            // minute; the build uuid keys the dedup so replay never double-bills.
            let duration_ms = build.duration_ms.unwrap_or(0).max(0);
            let minutes = if duration_ms == 0 {
                1
            } else {
                (duration_ms + 59_999) / 60_000
            };
            let period_start = Self::billing_period_start(build.started_at.as_deref());
            self.record_usage(InsertUsageEventCommand {
                tenant_id: context.tenant_id,
                organization_id: context.organization_id.unwrap_or(0),
                app_id: None,
                binding_id: None,
                attribution: None,
                period_start,
                dimension: USAGE_DIMENSION_BUILD_MINUTES.to_owned(),
                quantity: minutes,
                unit: "MINUTES".to_owned(),
                source_target_uuid: Some(build.platform_target_id.clone()),
                source_window_id: Some(format!("build:{}", build.id)),
                deduplication_key: format!("build:{}", build.id),
            })
            .await;
        }
        Ok(build)
    }

    // -- packages --------------------------------------------------------------------

    pub async fn register_package(
        &self,
        context: &DeployAppRequestContext,
        request: &RegisterPackageRequest,
    ) -> DeployServiceResult<PackageResponse> {
        let tenant_id = Self::tenant_id(context)?;
        self.enforce_entitlement(tenant_id, ENTITLEMENT_DIMENSION_PACKAGE_STORAGE_BYTES)
            .await?;
        validate_sha256(&request.checksum_sha256, "checksumSha256")?;
        validate_sha256(&request.manifest_sha256, "manifestSha256")?;
        SemanticVersion::parse(&request.semantic_version)
            .map_err(|error| DeployServiceError::validation(format!("semanticVersion: {error}")))?;

        // Platform/format compatibility and size ceilings are enforced here
        // and again by the package validator on the byte boundary.
        let (app_uuid, target_uuid, platform) = self
            .repository
            .resolve_build_platform(tenant_id, &request.build_id)
            .await?;
        if target_uuid != request.platform_target_id {
            return Err(DeployServiceError::validation(
                "platform target does not match the build platform target",
            ));
        }
        validate_package_format_for_platform(&platform, request.package_format.as_str())
            .map_err(DeployServiceError::validation)?;
        validate_package_size(
            &platform,
            request.package_format.as_str(),
            request.package_size_bytes as u64,
        )
        .map_err(DeployServiceError::validation)?;

        let package = self
            .repository
            .register_package(tenant_id, context.actor_id, request)
            .await?;
        self.audit_app_action(context, "package.register", &package.id)
            .await?;
        // Stored package bytes bill storage; the package uuid keys dedup.
        self.record_usage(InsertUsageEventCommand {
            tenant_id: context.tenant_id,
            organization_id: context.organization_id.unwrap_or(0),
            app_id: None,
            binding_id: None,
            attribution: None,
            period_start: Self::billing_period_start(None),
            dimension: USAGE_DIMENSION_PACKAGE_STORAGE_BYTES.to_owned(),
            quantity: package.package_size_bytes.max(0),
            unit: "BYTES".to_owned(),
            source_target_uuid: Some(app_uuid),
            source_window_id: Some(format!("package:{}", package.id)),
            deduplication_key: format!("package:{}", package.id),
        })
        .await;
        Ok(package)
    }

    pub async fn list_packages(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<PackagePage> {
        let tenant_id = Self::tenant_id(context)?;
        self.repository
            .list_packages(tenant_id, app_id, page, page_size)
            .await
    }

    pub async fn retrieve_package(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        package_id: &str,
    ) -> DeployServiceResult<PackageResponse> {
        let tenant_id = Self::tenant_id(context)?;
        self.repository
            .retrieve_package(tenant_id, app_id, package_id)
            .await
    }

    // -- releases ----------------------------------------------------------------------

    pub async fn create_app_release(
        &self,
        context: &DeployAppRequestContext,
        request: &CreateAppReleaseRequest,
    ) -> DeployServiceResult<AppReleaseResponse> {
        let tenant_id = Self::tenant_id(context)?;
        self.enforce_entitlement(tenant_id, ENTITLEMENT_DIMENSION_RELEASE_COUNT)
            .await?;
        if request.idempotency_key.trim().is_empty() {
            return Err(DeployServiceError::validation("idempotencyKey is required"));
        }
        SemanticVersion::parse(&request.semantic_version)
            .map_err(|error| DeployServiceError::validation(format!("semanticVersion: {error}")))?;
        let release = self
            .repository
            .create_app_release(tenant_id, context.actor_id, request)
            .await?;
        self.audit_app_action(context, "release.create", &release.id)
            .await?;
        Ok(release)
    }

    pub async fn list_app_releases(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<AppReleasePage> {
        let tenant_id = Self::tenant_id(context)?;
        self.repository
            .list_app_releases(tenant_id, app_id, page, page_size)
            .await
    }

    pub async fn retrieve_app_release(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        release_id: &str,
    ) -> DeployServiceResult<AppReleaseResponse> {
        let tenant_id = Self::tenant_id(context)?;
        self.repository
            .retrieve_app_release(tenant_id, app_id, release_id)
            .await
    }

    pub async fn update_app_release_status(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        release_id: &str,
        release_status: ReleaseStatus,
    ) -> DeployServiceResult<AppReleaseResponse> {
        let tenant_id = Self::tenant_id(context)?;
        let release = self
            .repository
            .update_app_release_status(tenant_id, app_id, release_id, release_status)
            .await?;
        self.audit_app_action(context, "release.statusUpdate", &release.id)
            .await?;
        Ok(release)
    }

    // -- channels -----------------------------------------------------------------------

    pub async fn list_channels(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
    ) -> DeployServiceResult<ChannelPage> {
        let tenant_id = Self::tenant_id(context)?;
        self.repository.list_channels(tenant_id, app_id).await
    }

    pub async fn retrieve_channel(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        channel_id: &str,
    ) -> DeployServiceResult<ChannelResponse> {
        let tenant_id = Self::tenant_id(context)?;
        self.repository
            .retrieve_channel(tenant_id, app_id, channel_id)
            .await
    }

    pub async fn promote_channel(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        channel_id: &str,
        request: &PromoteChannelRequest,
    ) -> DeployServiceResult<ChannelRolloutResponse> {
        let tenant_id = Self::tenant_id(context)?;
        let rollout = self
            .repository
            .promote_channel(tenant_id, app_id, channel_id, context.actor_id, request)
            .await?;
        self.audit_app_action(context, "channel.promote", &rollout.id)
            .await?;
        Ok(rollout)
    }

    pub async fn list_channel_rollouts(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        channel_id: &str,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<ChannelRolloutPage> {
        let tenant_id = Self::tenant_id(context)?;
        self.repository
            .list_channel_rollouts(tenant_id, app_id, channel_id, page, page_size)
            .await
    }

    // -- deployments ----------------------------------------------------------------------

    pub async fn create_app_deployment(
        &self,
        context: &DeployAppRequestContext,
        request: &CreateAppDeploymentRequest,
    ) -> DeployServiceResult<sdkwork_deploy_contract::AppDeploymentResponse> {
        let tenant_id = Self::tenant_id(context)?;
        self.enforce_entitlement(tenant_id, ENTITLEMENT_DIMENSION_DEPLOYMENT_COUNT)
            .await?;
        if request.idempotency_key.trim().is_empty() {
            return Err(DeployServiceError::validation("idempotencyKey is required"));
        }
        validate_deployment_pair(request)?;
        let deployment = self
            .repository
            .create_app_deployment(tenant_id, context.actor_id, request)
            .await?;
        self.audit_app_action(context, "deployment.create", &deployment.id)
            .await?;
        // Idempotency replay returns the existing deployment, so the
        // deployment uuid keys dedup and never over-counts.
        self.record_usage(InsertUsageEventCommand {
            tenant_id: context.tenant_id,
            organization_id: context.organization_id.unwrap_or(0),
            app_id: None,
            binding_id: None,
            attribution: None,
            period_start: Self::billing_period_start(None),
            dimension: USAGE_DIMENSION_DEPLOYMENT_COUNT.to_owned(),
            quantity: 1,
            unit: "COUNT".to_owned(),
            source_target_uuid: Some(deployment.app_id.clone()),
            source_window_id: Some(format!("deployment:{}", deployment.id)),
            deduplication_key: format!("deployment:{}", deployment.id),
        })
        .await;
        Ok(deployment)
    }

    pub async fn list_app_deployments(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<sdkwork_deploy_contract::AppDeploymentPage> {
        let tenant_id = Self::tenant_id(context)?;
        self.repository
            .list_app_deployments(tenant_id, app_id, page, page_size)
            .await
    }

    pub async fn retrieve_app_deployment(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        deployment_id: &str,
    ) -> DeployServiceResult<sdkwork_deploy_contract::AppDeploymentResponse> {
        let tenant_id = Self::tenant_id(context)?;
        self.repository
            .retrieve_app_deployment(tenant_id, app_id, deployment_id)
            .await
    }

    pub async fn update_app_deployment_state(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        deployment_id: &str,
        deployment_status: DeploymentStatus,
        platform_review_ref: Option<&str>,
    ) -> DeployServiceResult<sdkwork_deploy_contract::AppDeploymentResponse> {
        let tenant_id = Self::tenant_id(context)?;
        let deployment = self
            .repository
            .update_app_deployment_state(
                tenant_id,
                app_id,
                deployment_id,
                deployment_status,
                platform_review_ref,
            )
            .await?;
        self.audit_app_action(context, "deployment.stateUpdate", &deployment.id)
            .await?;
        Ok(deployment)
    }

    // -- signing identities -----------------------------------------------------------------

    pub async fn create_signing_identity(
        &self,
        context: &DeployAppRequestContext,
        request: &CreateSigningIdentityRequest,
    ) -> DeployServiceResult<SigningIdentityResponse> {
        let tenant_id = Self::tenant_id(context)?;
        if request.identity_name.trim().is_empty() || request.identity_name.len() > 200 {
            return Err(DeployServiceError::validation(
                "identityName must be 1..=200 characters",
            ));
        }
        let identity = self
            .repository
            .create_signing_identity(tenant_id, context.actor_id, request)
            .await?;
        self.audit_app_action(context, "signingIdentity.create", &identity.id)
            .await?;
        Ok(identity)
    }

    pub async fn list_signing_identities(
        &self,
        context: &DeployAppRequestContext,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<SigningIdentityPage> {
        let tenant_id = Self::tenant_id(context)?;
        self.repository
            .list_signing_identities(tenant_id, page, page_size)
            .await
    }

    pub async fn retrieve_signing_identity(
        &self,
        context: &DeployAppRequestContext,
        identity_id: &str,
    ) -> DeployServiceResult<SigningIdentityResponse> {
        let tenant_id = Self::tenant_id(context)?;
        self.repository
            .retrieve_signing_identity(tenant_id, identity_id)
            .await
    }

    // -- usage metering (read-only tenant surface, TECH §4.6) ------------------

    pub async fn list_usage_events(
        &self,
        context: &DeployAppRequestContext,
        query: &UsageEventQuery,
    ) -> DeployServiceResult<UsageEventPage> {
        let tenant_id = Self::tenant_id(context)?;
        self.repository.list_usage_events(tenant_id, query).await
    }

    // -- application database structure contract -------------------------------

    pub async fn create_app_database_profile(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        request: &CreateAppDatabaseProfileRequest,
    ) -> DeployServiceResult<AppDatabaseProfileResponse> {
        let tenant_id = Self::tenant_id(context)?;
        validate_database_engine(&request.db_engine).map_err(DeployServiceError::validation)?;
        validate_catalog_name(&request.catalog_name).map_err(DeployServiceError::validation)?;
        validate_profile_key(&request.profile_key).map_err(DeployServiceError::validation)?;
        if let Some(strategy) = request.migration_strategy.as_deref() {
            validate_migration_strategy(strategy).map_err(DeployServiceError::validation)?;
        }
        let profile = self
            .repository
            .create_app_database_profile(tenant_id, context.actor_id, app_id, request)
            .await?;
        self.audit_app_action(context, "databaseProfile.create", &profile.id)
            .await?;
        Ok(profile)
    }

    pub async fn list_app_database_profiles(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<AppDatabaseProfilePage> {
        let tenant_id = Self::tenant_id(context)?;
        self.repository
            .list_app_database_profiles(tenant_id, app_id, page, page_size)
            .await
    }

    pub async fn retrieve_app_database_profile(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        profile_id: &str,
    ) -> DeployServiceResult<AppDatabaseProfileResponse> {
        let tenant_id = Self::tenant_id(context)?;
        self.repository
            .retrieve_app_database_profile(tenant_id, app_id, profile_id)
            .await
    }

    pub async fn update_app_database_profile(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        profile_id: &str,
        request: &UpdateAppDatabaseProfileRequest,
    ) -> DeployServiceResult<AppDatabaseProfileResponse> {
        let tenant_id = Self::tenant_id(context)?;
        if let Some(strategy) = request.migration_strategy.as_deref() {
            validate_migration_strategy(strategy).map_err(DeployServiceError::validation)?;
        }
        if let Some(status) = request.profile_status.as_deref() {
            validate_profile_status(status).map_err(DeployServiceError::validation)?;
        }
        let profile = self
            .repository
            .update_app_database_profile(tenant_id, context.actor_id, app_id, profile_id, request)
            .await?;
        self.audit_app_action(context, "databaseProfile.update", &profile.id)
            .await?;
        Ok(profile)
    }

    pub async fn create_app_database_migration(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        profile_id: &str,
        request: &CreateAppDatabaseMigrationRequest,
    ) -> DeployServiceResult<AppDatabaseMigrationResponse> {
        let tenant_id = Self::tenant_id(context)?;
        validate_migration_version(&request.migration_version)
            .map_err(DeployServiceError::validation)?;
        validate_migration_name(&request.migration_name).map_err(DeployServiceError::validation)?;
        validate_sha256(&request.checksum_sha256, "checksumSha256")?;
        let migration = self
            .repository
            .create_app_database_migration(tenant_id, context.actor_id, app_id, profile_id, request)
            .await?;
        self.audit_app_action(context, "databaseMigration.create", &migration.id)
            .await?;
        Ok(migration)
    }

    pub async fn list_app_database_migrations(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        profile_id: &str,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<AppDatabaseMigrationPage> {
        let tenant_id = Self::tenant_id(context)?;
        self.repository
            .list_app_database_migrations(tenant_id, app_id, profile_id, page, page_size)
            .await
    }

    pub async fn retrieve_app_database_migration(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        profile_id: &str,
        migration_id: &str,
    ) -> DeployServiceResult<AppDatabaseMigrationResponse> {
        let tenant_id = Self::tenant_id(context)?;
        self.repository
            .retrieve_app_database_migration(tenant_id, app_id, profile_id, migration_id)
            .await
    }
    // -- application environments and promotion chain -----------------------------

    pub async fn create_app_environment(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        request: &CreateAppEnvironmentRequest,
    ) -> DeployServiceResult<AppEnvironmentResponse> {
        let tenant_id = Self::tenant_id(context)?;
        validate_environment_key(&request.env_key)?;
        if request.env_name.trim().is_empty() || request.env_name.len() > 100 {
            return Err(DeployServiceError::validation(
                "envName must be 1..=100 characters",
            ));
        }
        if !matches!(
            request.env_level.as_str(),
            "DEVELOPMENT" | "STAGING" | "PRODUCTION"
        ) {
            return Err(DeployServiceError::validation(
                "envLevel must be DEVELOPMENT, STAGING, or PRODUCTION",
            ));
        }
        let environment = self
            .repository
            .create_app_environment(tenant_id, context.actor_id, app_id, request)
            .await?;
        self.audit_app_action(context, "environment.create", &environment.id)
            .await?;
        Ok(environment)
    }

    pub async fn list_app_environments(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<AppEnvironmentPage> {
        let tenant_id = Self::tenant_id(context)?;
        self.repository
            .list_app_environments(tenant_id, app_id, page, page_size)
            .await
    }

    pub async fn retrieve_app_environment(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        environment_id: &str,
    ) -> DeployServiceResult<AppEnvironmentResponse> {
        let tenant_id = Self::tenant_id(context)?;
        self.repository
            .retrieve_app_environment(tenant_id, app_id, environment_id)
            .await
    }

    pub async fn update_app_environment(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        environment_id: &str,
        request: &UpdateAppEnvironmentRequest,
    ) -> DeployServiceResult<AppEnvironmentResponse> {
        let tenant_id = Self::tenant_id(context)?;
        if let Some(status) = request.env_status.as_deref() {
            if !matches!(status, "DRAFT" | "ACTIVE" | "ARCHIVED") {
                return Err(DeployServiceError::validation(
                    "envStatus must be DRAFT, ACTIVE, or ARCHIVED",
                ));
            }
        }
        if let Some(name) = request.env_name.as_deref() {
            if name.trim().is_empty() || name.len() > 100 {
                return Err(DeployServiceError::validation(
                    "envName must be 1..=100 characters",
                ));
            }
        }
        let environment = self
            .repository
            .update_app_environment(tenant_id, context.actor_id, app_id, environment_id, request)
            .await?;
        self.audit_app_action(context, "environment.update", &environment.id)
            .await?;
        Ok(environment)
    }

    pub async fn promote_environment(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        environment_id: &str,
        request: &PromoteEnvironmentRequest,
    ) -> DeployServiceResult<EnvironmentPromotionResponse> {
        let tenant_id = Self::tenant_id(context)?;
        if request.release_id.trim().is_empty() {
            return Err(DeployServiceError::validation("releaseId is required"));
        }
        if let Some(note) = request.note.as_deref() {
            if note.len() > 500 {
                return Err(DeployServiceError::validation(
                    "note must be at most 500 characters",
                ));
            }
        }
        let promotion = self
            .repository
            .promote_environment(tenant_id, context.actor_id, app_id, environment_id, request)
            .await?;
        self.audit_app_action(context, "environment.promote", &promotion.id)
            .await?;
        Ok(promotion)
    }

    pub async fn list_environment_promotions(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        environment_id: &str,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<EnvironmentPromotionPage> {
        let tenant_id = Self::tenant_id(context)?;
        self.repository
            .list_environment_promotions(tenant_id, app_id, environment_id, page, page_size)
            .await
    }
} // ---------------------------------------------------------------------------
  // Validation helpers
  // ---------------------------------------------------------------------------

fn validate_environment_key(key: &str) -> DeployServiceResult<()> {
    if key.is_empty() || key.len() > 32 {
        return Err(DeployServiceError::validation(
            "envKey must be 1..=32 characters",
        ));
    }
    if !key
        .bytes()
        .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    {
        return Err(DeployServiceError::validation(
            "envKey must be lowercase letters, digits, and hyphens",
        ));
    }
    Ok(())
}

fn is_bounded_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 120
        && slug
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn validate_channel_keys(channels: &[String]) -> DeployServiceResult<()> {
    if channels.is_empty() || channels.len() > 8 {
        return Err(DeployServiceError::validation(
            "allowedChannels must contain 1..=8 channel keys",
        ));
    }
    for channel in channels {
        if ChannelKey::parse(channel).is_none() {
            return Err(DeployServiceError::validation(format!(
                "channel key {channel} must be one of {}",
                CHANNEL_KEYS.join(", ")
            )));
        }
    }
    Ok(())
}

fn validate_repo_url(url: &str) -> DeployServiceResult<()> {
    if url.is_empty() || url.len() > 1000 {
        return Err(DeployServiceError::validation(
            "repoUrl must be 1..=1000 characters",
        ));
    }
    if url.contains("://") {
        if !(url.starts_with("https://") || url.starts_with("http://")) {
            return Err(DeployServiceError::validation(
                "repoUrl must use https:// or http://",
            ));
        }
        if let Some(credentials_end) = url.find("://") {
            let scheme = &url[..credentials_end + 3];
            let rest = &url[credentials_end + 3..];
            if rest.contains('@') && !rest.starts_with("git@") {
                return Err(DeployServiceError::validation(
                    "repoUrl must not embed credentials",
                ));
            }
            let _ = scheme;
        }
    } else if !url.starts_with("git@") {
        return Err(DeployServiceError::validation(
            "repoUrl must be an https://, http://, or git@ scp-style URL",
        ));
    }
    Ok(())
}

fn validate_sha256(value: &str, field: &str) -> DeployServiceResult<()> {
    validate_sha256_hex(value, field).map_err(DeployServiceError::validation)
}

fn validate_deployment_pair(request: &CreateAppDeploymentRequest) -> DeployServiceResult<()> {
    let compatible = matches!(
        (
            request.deployment_kind.as_str(),
            request.deployment_target.as_str()
        ),
        ("MINIPROGRAM_REVIEW", "WECHAT_REVIEW" | "DOUYIN_REVIEW")
            | (
                "STORE_SUBMISSION",
                "APP_STORE_CONNECT" | "TESTFLIGHT" | "HARMONYOS_STORE"
            )
            | ("OTA_DISTRIBUTION", "OTA")
            | ("ENTERPRISE_DISTRIBUTION", "ENTERPRISE")
            | ("CONTAINER_ROLLOUT", "CONTAINER")
            | ("ARTIFACT_RELEASE", "WEB_NODE")
            | ("SITE_CONFIG", "WEB_NODE")
            | ("TLS_CONFIG", "WEB_NODE")
    );
    if !compatible {
        return Err(DeployServiceError::validation(format!(
            "deployment kind {} is not compatible with target {}",
            request.deployment_kind.as_str(),
            request.deployment_target.as_str()
        )));
    }
    Ok(())
}

#[cfg(test)]
mod usage_metering_tests {
    use chrono::{Datelike, Timelike};

    use super::DeployService;

    fn period_start(at: Option<&str>) -> String {
        DeployService::billing_period_start(at)
    }

    #[test]
    fn billing_period_start_truncates_to_utc_month_start() {
        assert_eq!(
            period_start(Some("2026-08-15T13:45:30.123Z")),
            "2026-08-01T00:00:00.000Z"
        );
        assert_eq!(
            period_start(Some("2026-01-01T00:00:00.000Z")),
            "2026-01-01T00:00:00.000Z"
        );
    }

    #[test]
    fn billing_period_start_falls_back_to_current_month() {
        let now = chrono::Utc::now();
        let period = period_start(None);
        let parsed = chrono::DateTime::parse_from_rfc3339(&period)
            .expect("fallback period is a valid timestamp");
        assert_eq!(parsed.day(), 1);
        assert_eq!(parsed.hour(), 0);
        assert_eq!(parsed.minute(), 0);
        assert_eq!(parsed.year(), now.year());
        assert_eq!(parsed.month(), now.month());
        // Malformed input behaves like missing input.
        let fallback = period_start(Some("not-a-date"));
        assert_eq!(fallback, period);
    }
}
