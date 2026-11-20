//! Channel release repository against a real PostgreSQL instance.
//!
//! Proves the persistence contract: creation is scoped to an existing resource
//! link in the same environment, the transition state machine runs in the
//! database transaction, and a stale version is rejected instead of overwriting
//! a newer provider observation.
//!
//! Requires `SDKWORK_DATABASE_TEST_POSTGRES_URL`.
mod common;

use sdkwork_deploy_contract::{
    ChannelAccessMode, ChannelKind, ChannelReleaseStatus, CreateChannelReleaseRequest,
};
use sdkwork_intelligence_deploy_service::ChannelReleaseStore;
use sqlx::PgPool;

const APP_UUID: &str = "11111111-1111-4111-8111-111111111111";
const LINK_ID: &str = "22222222-2222-4222-8222-222222222222";

async fn seed_app_and_link(pool: &PgPool) {
    sqlx::raw_sql(
        "INSERT INTO deploy_app (id, uuid, tenant_id, organization_id, name, slug, app_kind,
                                 app_status, type, runtime_config, metadata, data_scope,
                                 default_environment, created_at, updated_at, version)
         VALUES (7001, '11111111-1111-4111-8111-111111111111', 7, 0, 'Channel App',
                 'channel-app', 'WECHAT_MINIPROGRAM', 'ACTIVE', 1, '{}', '{}', 1,
                 'development', NOW(), NOW(), 1);

         INSERT INTO deploy_application_resource_link
             (id, tenant_id, organization_id, app_id, env_key, resource_kind, resource_ref,
              provider_code, binding_status)
         VALUES ('22222222-2222-4222-8222-222222222222', 7, 0, 7001, 'development',
                 'mini_program', 'wxappid', 'wechat', 'ACTIVE');",
    )
    .execute(pool)
    .await
    .expect("seed app and resource link");
}

fn create_request() -> CreateChannelReleaseRequest {
    CreateChannelReleaseRequest {
        environment: "development".to_string(),
        resource_link_id: LINK_ID.to_string(),
        channel: ChannelKind::WechatMiniProgram,
        access_mode: ChannelAccessMode::OperatorAuthorized,
        template_id: Some("template-1".to_string()),
        provider_version: Some("1.0.0".to_string()),
        ext_json: None,
    }
}

#[tokio::test]
async fn creates_and_transitions_a_channel_release() {
    let (repository, pool) = common::migrated_repository_with_pool("channel-release-repo").await;
    seed_app_and_link(&pool).await;

    let created = repository
        .create_channel_release_repo(7, None, APP_UUID, &create_request())
        .await
        .expect("create channel release");
    assert_eq!(created.release_status, ChannelReleaseStatus::Draft);
    assert_eq!(created.channel, ChannelKind::WechatMiniProgram);
    assert_eq!(created.access_mode, ChannelAccessMode::OperatorAuthorized);
    assert_eq!(created.version, "1");
    assert_eq!(created.template_id.as_deref(), Some("template-1"));

    let listed = repository
        .list_channel_releases_repo(7, APP_UUID, Some("development"))
        .await
        .expect("list");
    assert_eq!(listed.len(), 1);

    let projected = repository
        .transition_channel_release_repo(
            7,
            &created.id,
            1,
            ChannelReleaseStatus::Projected,
            None,
            None,
            None,
        )
        .await
        .expect("transition to projected");
    assert_eq!(projected.release_status, ChannelReleaseStatus::Projected);
    assert_eq!(projected.version, "2");

    let rejected = repository
        .transition_channel_release_repo(
            7,
            &created.id,
            2,
            ChannelReleaseStatus::Released,
            None,
            None,
            None,
        )
        .await
        .expect_err("shortcut DRAFT->RELEASED must be rejected");
    assert!(
        format!("{rejected}").contains("invalid channel release transition"),
        "unexpected error: {rejected}"
    );

    let stale = repository
        .transition_channel_release_repo(
            7,
            &created.id,
            1,
            ChannelReleaseStatus::Uploaded,
            None,
            None,
            None,
        )
        .await
        .expect_err("stale version must be rejected");
    assert!(
        format!("{stale}").contains("version changed"),
        "unexpected error: {stale}"
    );
}

#[tokio::test]
async fn repository_store_adapter_matches_the_state_machine() {
    let (repository, pool) = common::migrated_repository_with_pool("channel-release-store").await;
    seed_app_and_link(&pool).await;
    let created = repository
        .create_channel_release_repo(7, None, APP_UUID, &create_request())
        .await
        .expect("create channel release");

    let (status, version) = repository
        .load_channel_release_status(7, &created.id)
        .await
        .expect("load status");
    assert_eq!(status, ChannelReleaseStatus::Draft);
    assert_eq!(version, 1);

    repository
        .transition_channel_release(
            7,
            &created.id,
            1,
            ChannelReleaseStatus::Projected,
            Some("projected"),
        )
        .await
        .expect("apply observed transition");
    let (status, version) = repository
        .load_channel_release_status(7, &created.id)
        .await
        .expect("reload status");
    assert_eq!(status, ChannelReleaseStatus::Projected);
    assert_eq!(version, 2);

    let error = repository
        .transition_channel_release(7, &created.id, 2, ChannelReleaseStatus::Released, None)
        .await
        .expect_err("unreachable observation must be rejected");
    assert!(format!("{error}").contains("invalid channel release transition"));
}

#[tokio::test]
async fn refuses_a_missing_resource_link() {
    let (repository, pool) = common::migrated_repository_with_pool("channel-release-link").await;
    seed_app_and_link(&pool).await;
    let mut request = create_request();
    request.resource_link_id = "33333333-3333-4333-8333-333333333333".to_string();
    let error = repository
        .create_channel_release_repo(7, None, APP_UUID, &request)
        .await
        .expect_err("missing link must be refused");
    assert!(
        format!("{error}").contains("resource link"),
        "unexpected error: {error}"
    );
}
