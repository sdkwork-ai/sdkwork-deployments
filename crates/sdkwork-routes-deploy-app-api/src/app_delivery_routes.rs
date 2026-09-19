//! Unified app delivery route handlers (REQ-2026-0002): apps, platform
//! targets, source repositories, build templates, builds, packages, releases,
//! channels, rollouts, deployments, and signing identities.

use std::collections::HashMap;

use axum::{
    extract::{Path, Query, State},
    http::HeaderMap,
    response::Response,
    routing::{get, patch, post},
    Extension, Json, Router,
};
use sdkwork_deploy_contract::{
    CreateAppDatabaseMigrationRequest, CreateAppDatabaseProfileRequest, CreateAppDeploymentRequest,
    CreateAppEnvironmentRequest, CreateAppReleaseRequest, CreateAppRequest, CreateBuildRequest,
    CreateBuildTemplateRequest, CreatePlatformTargetRequest, CreateSigningIdentityRequest,
    CreateSourceRepositoryRequest, DeployAppRequestContext, PromoteChannelRequest,
    PromoteEnvironmentRequest, RegisterPackageRequest, UpdateAppDatabaseProfileRequest,
    UpdateAppEnvironmentRequest, UpdateAppReleaseStatusRequest, UpdateAppRequest,
    UpdateBuildStateRequest, UsageEventQuery,
};
use sdkwork_routes_deploy_common::{envelope, finish_api_json, finish_created_api_json, ok_json};
use sdkwork_web_core::WebRequestContext;
use serde::Deserialize;

use crate::{auth::require_app_context, paths, routes::required_header, routes::AppState};

#[derive(Deserialize)]
struct PageQuery {
    page: Option<i32>,
    page_size: Option<i32>,
}

fn page_values(query: &PageQuery) -> (i32, i32) {
    (query.page.unwrap_or(1), query.page_size.unwrap_or(20))
}

/// Composable unified app delivery router block.
pub fn build_app_delivery_router() -> Router<AppState> {
    Router::<AppState>::new()
        .route(paths::APPS, get(list_apps).post(create_app))
        .route(paths::APP, get(retrieve_app).patch(update_app))
        .route(paths::APP_DOMAINS, get(list_app_domains))
        .route(
            paths::APP_PLATFORM_TARGETS,
            get(list_platform_targets).post(create_platform_target),
        )
        .route(paths::APP_PLATFORM_TARGET, get(retrieve_platform_target))
        .route(
            paths::APP_SOURCE_REPOSITORIES,
            get(list_source_repositories).post(create_source_repository),
        )
        .route(
            paths::APP_SOURCE_REPOSITORY,
            get(retrieve_source_repository),
        )
        .route(
            paths::BUILD_TEMPLATES,
            get(list_build_templates).post(create_build_template),
        )
        .route(paths::BUILD_TEMPLATE, get(retrieve_build_template))
        .route(paths::APP_BUILDS, get(list_builds).post(create_build))
        .route(paths::APP_BUILD, get(retrieve_build))
        .route(paths::APP_BUILD_STATE, patch(update_build_state))
        .route(
            paths::APP_PACKAGES,
            get(list_packages).post(register_package),
        )
        .route(paths::APP_PACKAGE, get(retrieve_package))
        .route(
            paths::APP_RELEASES,
            get(list_app_releases).post(create_app_release),
        )
        .route(
            paths::APP_RELEASE,
            get(retrieve_app_release).patch(update_app_release_status),
        )
        .route(paths::APP_CHANNELS, get(list_channels))
        .route(paths::APP_CHANNEL, get(retrieve_channel))
        .route(paths::APP_CHANNEL_PROMOTIONS, post(promote_channel))
        .route(paths::APP_CHANNEL_ROLLOUTS, get(list_channel_rollouts))
        .route(
            paths::APP_DEPLOYMENTS,
            get(list_app_deployments).post(create_app_deployment),
        )
        .route(paths::APP_DEPLOYMENT, get(retrieve_app_deployment))
        .route(
            paths::SIGNING_IDENTITIES,
            get(list_signing_identities).post(create_signing_identity),
        )
        .route(paths::SIGNING_IDENTITY, get(retrieve_signing_identity))
        .route(paths::USAGE_EVENTS, get(list_usage_events))
        .route(
            paths::APP_DATABASE_PROFILES,
            get(list_app_database_profiles).post(create_app_database_profile),
        )
        .route(
            paths::APP_DATABASE_PROFILE,
            get(retrieve_app_database_profile).patch(update_app_database_profile),
        )
        .route(
            paths::APP_DATABASE_MIGRATIONS,
            get(list_app_database_migrations).post(create_app_database_migration),
        )
        .route(
            paths::APP_DATABASE_MIGRATION,
            get(retrieve_app_database_migration),
        )
        .route(
            paths::APP_ENVIRONMENTS,
            get(list_app_environments).post(create_app_environment),
        )
        .route(
            paths::APP_ENVIRONMENT,
            get(retrieve_app_environment).patch(update_app_environment),
        )
        .route(
            paths::APP_ENVIRONMENT_PROMOTIONS,
            get(list_environment_promotions).post(promote_environment),
        )
}

// -- apps ------------------------------------------------------------------

async fn list_apps(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    Query(query): Query<PageQuery>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = require_app_context(context)?;
            let (page, page_size) = page_values(&query);
            let result = state.api.list_apps(&context, page, page_size).await?;
            ok_json(envelope::app_page(result))
        }
        .await,
    )
}

async fn create_app(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    headers: HeaderMap,
    Json(request): Json<CreateAppRequest>,
) -> Response {
    finish_created_api_json(
        &ctx,
        async {
            let context = require_app_context(context)?;
            // `apps.create` is declared `x-sdkwork-idempotent: true` and
            // `IdempotencyKeyParam` is `required: true`, so the header is
            // validated here (422 naming the header) and forwarded to the
            // repository, which replays the original row for a repeated key
            // instead of colliding on `uk_deploy_app_tenant_slug`.
            let idempotency_key = required_header(&headers, "idempotency-key")?;
            let result = state
                .api
                .create_app(&context, Some(&idempotency_key), &request)
                .await?;
            ok_json(envelope::resource(result))
        }
        .await,
    )
}

async fn retrieve_app(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    Path(app_id): Path<String>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = require_app_context(context)?;
            let result = state.api.retrieve_app(&context, &app_id).await?;
            ok_json(envelope::resource(result))
        }
        .await,
    )
}

async fn update_app(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    Path(app_id): Path<String>,
    Json(request): Json<UpdateAppRequest>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = require_app_context(context)?;
            let result = state.api.update_app(&context, &app_id, &request).await?;
            ok_json(envelope::resource(result))
        }
        .await,
    )
}

async fn list_app_domains(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    Path(app_id): Path<String>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = require_app_context(context)?;
            let result = state.api.list_app_domains(&context, &app_id).await?;
            ok_json(envelope::app_domain_page(result))
        }
        .await,
    )
}

// -- platform targets ---------------------------------------------------------

async fn list_platform_targets(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    Path(app_id): Path<String>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = require_app_context(context)?;
            let result = state.api.list_platform_targets(&context, &app_id).await?;
            ok_json(envelope::platform_target_page(result))
        }
        .await,
    )
}

async fn create_platform_target(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    Path(app_id): Path<String>,
    Json(request): Json<CreatePlatformTargetRequest>,
) -> Response {
    finish_created_api_json(
        &ctx,
        async {
            let context = require_app_context(context)?;
            let result = state
                .api
                .create_platform_target(&context, &app_id, &request)
                .await?;
            ok_json(envelope::resource(result))
        }
        .await,
    )
}

async fn retrieve_platform_target(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    Path((app_id, target_id)): Path<(String, String)>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = require_app_context(context)?;
            let result = state
                .api
                .retrieve_platform_target(&context, &app_id, &target_id)
                .await?;
            ok_json(envelope::resource(result))
        }
        .await,
    )
}

// -- source repositories --------------------------------------------------------

async fn list_source_repositories(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    Path(app_id): Path<String>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = require_app_context(context)?;
            let result = state
                .api
                .list_source_repositories(&context, &app_id)
                .await?;
            ok_json(envelope::source_repository_page(result))
        }
        .await,
    )
}

async fn create_source_repository(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    Path(app_id): Path<String>,
    Json(request): Json<CreateSourceRepositoryRequest>,
) -> Response {
    finish_created_api_json(
        &ctx,
        async {
            let context = require_app_context(context)?;
            let result = state
                .api
                .create_source_repository(&context, &app_id, &request)
                .await?;
            ok_json(envelope::resource(result))
        }
        .await,
    )
}

async fn retrieve_source_repository(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    Path((app_id, repo_id)): Path<(String, String)>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = require_app_context(context)?;
            let result = state
                .api
                .retrieve_source_repository(&context, &app_id, &repo_id)
                .await?;
            ok_json(envelope::resource(result))
        }
        .await,
    )
}

// -- build templates ------------------------------------------------------------

async fn list_build_templates(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    Query(query): Query<PageQuery>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = require_app_context(context)?;
            let (page, page_size) = page_values(&query);
            let result = state
                .api
                .list_build_templates(&context, page, page_size)
                .await?;
            ok_json(envelope::build_template_page(result))
        }
        .await,
    )
}

async fn create_build_template(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    Json(request): Json<CreateBuildTemplateRequest>,
) -> Response {
    finish_created_api_json(
        &ctx,
        async {
            let context = require_app_context(context)?;
            let result = state.api.create_build_template(&context, &request).await?;
            ok_json(envelope::resource(result))
        }
        .await,
    )
}

async fn retrieve_build_template(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    Path(template_id): Path<String>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = require_app_context(context)?;
            let result = state
                .api
                .retrieve_build_template(&context, &template_id)
                .await?;
            ok_json(envelope::resource(result))
        }
        .await,
    )
}

// -- builds -------------------------------------------------------------------

async fn list_builds(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    Path(app_id): Path<String>,
    Query(query): Query<PageQuery>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = require_app_context(context)?;
            let (page, page_size) = page_values(&query);
            let result = state
                .api
                .list_builds(&context, &app_id, page, page_size)
                .await?;
            ok_json(envelope::build_page(result))
        }
        .await,
    )
}

async fn create_build(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    Json(request): Json<CreateBuildRequest>,
) -> Response {
    finish_created_api_json(
        &ctx,
        async {
            let context = require_app_context(context)?;
            let result = state.api.create_build(&context, &request).await?;
            ok_json(envelope::resource(result))
        }
        .await,
    )
}

async fn retrieve_build(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    Path((app_id, build_id)): Path<(String, String)>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = require_app_context(context)?;
            let result = state
                .api
                .retrieve_build(&context, &app_id, &build_id)
                .await?;
            ok_json(envelope::resource(result))
        }
        .await,
    )
}

async fn update_build_state(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    Path((app_id, build_id)): Path<(String, String)>,
    Json(request): Json<UpdateBuildStateRequest>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = require_app_context(context)?;
            let result = state
                .api
                .update_build_state(&context, &app_id, &build_id, &request)
                .await?;
            ok_json(envelope::resource(result))
        }
        .await,
    )
}

// -- packages -----------------------------------------------------------------

async fn list_packages(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    Path(app_id): Path<String>,
    Query(query): Query<PageQuery>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = require_app_context(context)?;
            let (page, page_size) = page_values(&query);
            let result = state
                .api
                .list_packages(&context, &app_id, page, page_size)
                .await?;
            ok_json(envelope::package_page(result))
        }
        .await,
    )
}

async fn register_package(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    Json(request): Json<RegisterPackageRequest>,
) -> Response {
    finish_created_api_json(
        &ctx,
        async {
            let context = require_app_context(context)?;
            let result = state.api.register_package(&context, &request).await?;
            ok_json(envelope::resource(result))
        }
        .await,
    )
}

async fn retrieve_package(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    Path((app_id, package_id)): Path<(String, String)>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = require_app_context(context)?;
            let result = state
                .api
                .retrieve_package(&context, &app_id, &package_id)
                .await?;
            ok_json(envelope::resource(result))
        }
        .await,
    )
}

// -- releases -------------------------------------------------------------------

async fn list_app_releases(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    Path(app_id): Path<String>,
    Query(query): Query<PageQuery>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = require_app_context(context)?;
            let (page, page_size) = page_values(&query);
            let result = state
                .api
                .list_app_releases(&context, &app_id, page, page_size)
                .await?;
            ok_json(envelope::app_release_page(result))
        }
        .await,
    )
}

async fn create_app_release(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    Json(request): Json<CreateAppReleaseRequest>,
) -> Response {
    finish_created_api_json(
        &ctx,
        async {
            let context = require_app_context(context)?;
            let result = state.api.create_app_release(&context, &request).await?;
            ok_json(envelope::resource(result))
        }
        .await,
    )
}

async fn retrieve_app_release(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    Path((app_id, release_id)): Path<(String, String)>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = require_app_context(context)?;
            let result = state
                .api
                .retrieve_app_release(&context, &app_id, &release_id)
                .await?;
            ok_json(envelope::resource(result))
        }
        .await,
    )
}

// -- channels ---------------------------------------------------------------------

async fn update_app_release_status(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    Path(params): Path<HashMap<String, String>>,
    Json(request): Json<UpdateAppReleaseStatusRequest>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = require_app_context(context)?;
            let app_id = params.get("appId").cloned().ok_or_else(|| {
                sdkwork_deploy_contract::DeployServiceError::validation("appId is required")
            })?;
            let release_id = params.get("releaseId").cloned().ok_or_else(|| {
                sdkwork_deploy_contract::DeployServiceError::validation("releaseId is required")
            })?;
            let result = state
                .api
                .update_app_release_status(&context, &app_id, &release_id, request.release_status)
                .await?;
            ok_json(envelope::resource(result))
        }
        .await,
    )
}

async fn list_channels(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    Path(app_id): Path<String>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = require_app_context(context)?;
            let result = state.api.list_channels(&context, &app_id).await?;
            ok_json(envelope::channel_page(result))
        }
        .await,
    )
}

async fn retrieve_channel(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    Path((app_id, channel_id)): Path<(String, String)>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = require_app_context(context)?;
            let result = state
                .api
                .retrieve_channel(&context, &app_id, &channel_id)
                .await?;
            ok_json(envelope::resource(result))
        }
        .await,
    )
}

async fn promote_channel(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    Path((app_id, channel_id)): Path<(String, String)>,
    Json(request): Json<PromoteChannelRequest>,
) -> Response {
    finish_created_api_json(
        &ctx,
        async {
            let context = require_app_context(context)?;
            let result = state
                .api
                .promote_channel(&context, &app_id, &channel_id, &request)
                .await?;
            ok_json(envelope::resource(result))
        }
        .await,
    )
}

async fn list_channel_rollouts(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    Path((app_id, channel_id)): Path<(String, String)>,
    Query(query): Query<PageQuery>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = require_app_context(context)?;
            let (page, page_size) = page_values(&query);
            let result = state
                .api
                .list_channel_rollouts(&context, &app_id, &channel_id, page, page_size)
                .await?;
            ok_json(envelope::channel_rollout_page(result))
        }
        .await,
    )
}

// -- deployments ---------------------------------------------------------------------

async fn list_app_deployments(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    Path(app_id): Path<String>,
    Query(query): Query<PageQuery>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = require_app_context(context)?;
            let (page, page_size) = page_values(&query);
            let result = state
                .api
                .list_app_deployments(&context, &app_id, page, page_size)
                .await?;
            ok_json(envelope::app_deployment_page(result))
        }
        .await,
    )
}

async fn create_app_deployment(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    Json(request): Json<CreateAppDeploymentRequest>,
) -> Response {
    finish_created_api_json(
        &ctx,
        async {
            let context = require_app_context(context)?;
            let result = state.api.create_app_deployment(&context, &request).await?;
            ok_json(envelope::resource(result))
        }
        .await,
    )
}

async fn retrieve_app_deployment(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    Path((app_id, deployment_id)): Path<(String, String)>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = require_app_context(context)?;
            let result = state
                .api
                .retrieve_app_deployment(&context, &app_id, &deployment_id)
                .await?;
            ok_json(envelope::resource(result))
        }
        .await,
    )
}

// -- signing identities ----------------------------------------------------------------

async fn list_signing_identities(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    Query(query): Query<PageQuery>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = require_app_context(context)?;
            let (page, page_size) = page_values(&query);
            let result = state
                .api
                .list_signing_identities(&context, page, page_size)
                .await?;
            ok_json(envelope::signing_identity_page(result))
        }
        .await,
    )
}

async fn create_signing_identity(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    Json(request): Json<CreateSigningIdentityRequest>,
) -> Response {
    finish_created_api_json(
        &ctx,
        async {
            let context = require_app_context(context)?;
            let result = state
                .api
                .create_signing_identity(&context, &request)
                .await?;
            ok_json(envelope::resource(result))
        }
        .await,
    )
}

async fn retrieve_signing_identity(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    Path(identity_id): Path<String>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = require_app_context(context)?;
            let result = state
                .api
                .retrieve_signing_identity(&context, &identity_id)
                .await?;
            ok_json(envelope::resource(result))
        }
        .await,
    )
}

// -- usage metering -----------------------------------------------------------

async fn list_usage_events(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    Query(query): Query<UsageEventQuery>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = require_app_context(context)?;
            let result = state.api.list_usage_events(&context, &query).await?;
            ok_json(envelope::usage_event_page(result))
        }
        .await,
    )
}

// -- application database structure contract ----------------------------------

async fn create_app_database_profile(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    Path(app_id): Path<String>,
    Json(request): Json<CreateAppDatabaseProfileRequest>,
) -> Response {
    finish_created_api_json(
        &ctx,
        async {
            let context = require_app_context(context)?;
            let result = state
                .api
                .create_app_database_profile(&context, &app_id, &request)
                .await?;
            ok_json(envelope::resource(result))
        }
        .await,
    )
}

async fn list_app_database_profiles(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    Path(app_id): Path<String>,
    Query(query): Query<PageQuery>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = require_app_context(context)?;
            let (page, page_size) = page_values(&query);
            let result = state
                .api
                .list_app_database_profiles(&context, &app_id, page, page_size)
                .await?;
            ok_json(envelope::app_database_profile_page(result))
        }
        .await,
    )
}

async fn retrieve_app_database_profile(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    Path((app_id, profile_id)): Path<(String, String)>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = require_app_context(context)?;
            let result = state
                .api
                .retrieve_app_database_profile(&context, &app_id, &profile_id)
                .await?;
            ok_json(envelope::resource(result))
        }
        .await,
    )
}

async fn update_app_database_profile(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    Path((app_id, profile_id)): Path<(String, String)>,
    Json(request): Json<UpdateAppDatabaseProfileRequest>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = require_app_context(context)?;
            let result = state
                .api
                .update_app_database_profile(&context, &app_id, &profile_id, &request)
                .await?;
            ok_json(envelope::resource(result))
        }
        .await,
    )
}

async fn create_app_database_migration(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    Path((app_id, profile_id)): Path<(String, String)>,
    Json(request): Json<CreateAppDatabaseMigrationRequest>,
) -> Response {
    finish_created_api_json(
        &ctx,
        async {
            let context = require_app_context(context)?;
            let result = state
                .api
                .create_app_database_migration(&context, &app_id, &profile_id, &request)
                .await?;
            ok_json(envelope::resource(result))
        }
        .await,
    )
}

async fn list_app_database_migrations(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    Path((app_id, profile_id)): Path<(String, String)>,
    Query(query): Query<PageQuery>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = require_app_context(context)?;
            let (page, page_size) = page_values(&query);
            let result = state
                .api
                .list_app_database_migrations(&context, &app_id, &profile_id, page, page_size)
                .await?;
            ok_json(envelope::app_database_migration_page(result))
        }
        .await,
    )
}

async fn retrieve_app_database_migration(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    Path((app_id, profile_id, migration_id)): Path<(String, String, String)>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = require_app_context(context)?;
            let result = state
                .api
                .retrieve_app_database_migration(&context, &app_id, &profile_id, &migration_id)
                .await?;
            ok_json(envelope::resource(result))
        }
        .await,
    )
}

// -- application environments and promotion chain ------------------------------

async fn create_app_environment(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    Path(app_id): Path<String>,
    Json(request): Json<CreateAppEnvironmentRequest>,
) -> Response {
    finish_created_api_json(
        &ctx,
        async {
            let context = require_app_context(context)?;
            let result = state
                .api
                .create_app_environment(&context, &app_id, &request)
                .await?;
            ok_json(envelope::resource(result))
        }
        .await,
    )
}

async fn list_app_environments(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    Path(app_id): Path<String>,
    Query(query): Query<PageQuery>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = require_app_context(context)?;
            let (page, page_size) = page_values(&query);
            let result = state
                .api
                .list_app_environments(&context, &app_id, page, page_size)
                .await?;
            ok_json(envelope::app_environment_page(result))
        }
        .await,
    )
}

async fn retrieve_app_environment(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    Path((app_id, environment_id)): Path<(String, String)>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = require_app_context(context)?;
            let result = state
                .api
                .retrieve_app_environment(&context, &app_id, &environment_id)
                .await?;
            ok_json(envelope::resource(result))
        }
        .await,
    )
}

async fn update_app_environment(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    Path((app_id, environment_id)): Path<(String, String)>,
    Json(request): Json<UpdateAppEnvironmentRequest>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = require_app_context(context)?;
            let result = state
                .api
                .update_app_environment(&context, &app_id, &environment_id, &request)
                .await?;
            ok_json(envelope::resource(result))
        }
        .await,
    )
}

async fn promote_environment(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    Path((app_id, environment_id)): Path<(String, String)>,
    Json(request): Json<PromoteEnvironmentRequest>,
) -> Response {
    finish_created_api_json(
        &ctx,
        async {
            let context = require_app_context(context)?;
            let result = state
                .api
                .promote_environment(&context, &app_id, &environment_id, &request)
                .await?;
            ok_json(envelope::resource(result))
        }
        .await,
    )
}

async fn list_environment_promotions(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    Path((app_id, environment_id)): Path<(String, String)>,
    Query(query): Query<PageQuery>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = require_app_context(context)?;
            let (page, page_size) = page_values(&query);
            let result = state
                .api
                .list_environment_promotions(&context, &app_id, &environment_id, page, page_size)
                .await?;
            ok_json(envelope::environment_promotion_page(result))
        }
        .await,
    )
}
