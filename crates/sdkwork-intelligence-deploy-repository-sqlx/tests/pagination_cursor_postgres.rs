//! Pagination keyset and secret-encryption integration tests
//! (PAGINATION_SPEC §6, SECURITY_SPEC secret-at-rest). Requires
//! `SDKWORK_DATABASE_TEST_POSTGRES_URL`; ignored by default like the other
//! PostgreSQL integration tests in this crate.

mod common;

use sdkwork_database_id::SnowflakeIdGenerator;
use sdkwork_deploy_contract::{
    AppKind, AuditLogQuery, CreateAppRequest, CreateEnvVariableRequest, DeployServiceErrorKind,
};
use sdkwork_intelligence_deploy_repository_sqlx::DeployRepository;
use sdkwork_intelligence_deploy_service::repository::InsertAuditLogCommand;
use sdkwork_intelligence_deploy_service::DeployRepositoryPort;
use sqlx::PgPool;

fn repository(pool: PgPool) -> DeployRepository {
    DeployRepository::new(
        pool,
        SnowflakeIdGenerator::new(6).expect("Snowflake generator"),
        common::test_secret_key(),
    )
}

/// Creates an app the way the console actually does: `apps.create` does not
/// declare `type`, so `app_type` is **omitted** (`None`) and the column default
/// must apply. Regression guard for the `deploy_app.type` NOT NULL violation —
/// binding an explicit NULL here used to fail every real create with an opaque
/// `insert deploy_app` 500. `metadata` is likewise sent by the real caller.
async fn create_app(repo: &DeployRepository, tenant_id: i64, name: &str) -> String {
    let response = repo
        .create_app(
            tenant_id,
            Some(0),
            Some(1),
            None,
            &CreateAppRequest {
                name: name.to_string(),
                slug: None,
                app_kind: AppKind::StaticWeb,
                app_type: None,
                runtime_config: None,
                metadata: Some(serde_json::json!({ "category": { "id": "cat-1" } })),
                description: None,
                default_environment: None,
                idempotency_key: None,
            },
        )
        .await
        .expect("create app");
    assert_eq!(
        response.app_type, 1,
        "omitted `type` must fall back to the column default, not NULL"
    );
    assert_eq!(
        response
            .metadata
            .as_ref()
            .and_then(|value| value.get("category"))
            .and_then(|value| value.get("id"))
            .and_then(|value| value.as_str()),
        Some("cat-1"),
        "`metadata` sent by the caller must be persisted into deploy_app.metadata"
    );
    response.id
}

/// 机密环境变量必须加密落库、响应掩码、列表永不明文（SECURITY_SPEC）。
#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn secret_env_variable_is_encrypted_at_rest_and_masked_in_responses() {
    let pool = common::postgres_pool().await;
    let repo = repository(pool.clone());
    let app_id = create_app(&repo, 7, "secret-app").await;

    let created = repo
        .create_env_variable(
            7,
            &app_id,
            &CreateEnvVariableRequest {
                environment: "production".to_string(),
                key: "API_TOKEN".to_string(),
                value: "super-secret-value".to_string(),
                is_secret: true,
            },
        )
        .await
        .expect("create secret env variable");
    // 响应必须掩码，绝不明文回传。
    assert_eq!(created.value, "***", "secret value must be masked");
    assert!(created.value != "super-secret-value");

    // 落库值必须是密文（base64(nonce||ciphertext)），不是明文也不是掩码。
    let stored: String =
        sqlx::query_scalar("SELECT value_encrypted FROM deploy_env_variable WHERE uuid = $1")
            .bind(&created.id)
            .fetch_one(&pool)
            .await
            .expect("read stored secret");
    assert_ne!(stored, "super-secret-value");
    assert_ne!(stored, "***");
    assert!(!stored.is_empty());

    // 列表返回掩码。
    let page = repo
        .list_env_variables(7, &app_id, Some("production"))
        .await
        .expect("list env variables");
    let listed = page
        .items
        .iter()
        .find(|item| item.id == created.id)
        .expect("created variable listed");
    assert_eq!(listed.value, "***", "listed secret must be masked");
}

/// 审计日志 keyset 翻页与无租户拒绝（fail-closed）。
#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn audit_logs_keyset_pages_and_requires_tenant() {
    let pool = common::postgres_pool().await;
    let repo = repository(pool.clone());

    for index in 0..5 {
        repo.insert_audit_log(InsertAuditLogCommand {
            tenant_id: 7,
            organization_id: 0,
            operator_id: 1,
            action: format!("action-{index}"),
            target_type: "app".to_string(),
            target_id: None,
            target_uuid: None,
        })
        .await
        .expect("insert audit log");
    }

    let query = AuditLogQuery {
        page_size: 2,
        ..AuditLogQuery::default()
    };
    let mut collected = Vec::new();
    let mut cursor: Option<String> = None;
    loop {
        let page = repo
            .list_audit_logs(Some(7), &query, cursor.as_deref())
            .await
            .expect("list audit page");
        collected.extend(page.items.iter().map(|item| item.id.clone()));
        match (page.has_more, page.next_cursor) {
            (Some(true), Some(next)) => cursor = Some(next),
            _ => break,
        }
    }
    assert_eq!(collected.len(), 5, "all audit rows collected");
    let unique: std::collections::HashSet<_> = collected.iter().collect();
    assert_eq!(unique.len(), 5, "no duplicate audit rows across pages");

    // 无租户上下文的调用必须被拒绝（跨租户越权防线）。
    let denied = repo
        .list_audit_logs(None, &query, None)
        .await
        .expect_err("tenant-less audit listing must be rejected");
    assert_eq!(denied.kind(), DeployServiceErrorKind::Forbidden);
}

/// How many apps the tenant currently has, for asserting a replay added none.
async fn count_apps(repo: &DeployRepository, tenant_id: i64) -> i64 {
    let page = repo.list_apps(tenant_id, 1, 100).await.expect("list apps");
    page.items.len() as i64
}

/// A create payload that mirrors what the console sends; `slug: None` lets the
/// server derive one, which is the path the Chinese-name regression exercises.
fn create_request(name: &str, slug: Option<&str>) -> CreateAppRequest {
    CreateAppRequest {
        name: name.to_string(),
        slug: slug.map(str::to_owned),
        app_kind: AppKind::StaticWeb,
        app_type: None,
        runtime_config: None,
        metadata: None,
        description: None,
        default_environment: None,
        idempotency_key: None,
    }
}

/// A non-ASCII name must never persist an empty slug.
///
/// `deploy_app.slug` is `NOT NULL` and unique per tenant, and every Chinese-only
/// name collapses to `""` under both the Rust and TypeScript slugifiers. Before
/// the guard, the *first* such app claimed `slug = ''` and every later one failed
/// the unique index with a permanent, unexplained `409 conflict: app slug
/// already exists in this tenant`. Two apps in one tenant is the smallest
/// reproduction of that sequence.
#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn non_ascii_names_get_distinct_generated_slugs() {
    let pool = common::postgres_pool().await;
    let repo = repository(pool.clone());

    let first = repo
        .create_app(9, Some(0), Some(1), None, &create_request("放大", None))
        .await
        .expect("first Chinese-named app must be creatable");
    let second = repo
        .create_app(9, Some(0), Some(1), None, &create_request("我的应用", None))
        .await
        .expect("second Chinese-named app must not collide on the first one's slug");

    assert_ne!(first.id, second.id);
    assert!(
        !first.slug.is_empty() && !second.slug.is_empty(),
        "generated slugs must never be empty: {:?} / {:?}",
        first.slug,
        second.slug
    );
    assert_ne!(
        first.slug, second.slug,
        "two distinct apps must not share a slug"
    );

    // The generated slug is what the default publishing domain falls back to, so
    // it has to be a legal DNS label.
    for slug in [&first.slug, &second.slug] {
        assert!(
            slug.bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-'),
            "slug {slug:?} is not a legal DNS label"
        );
    }
}

/// Replaying `apps.create` with the same `Idempotency-Key` returns the original
/// row instead of a `409`.
///
/// This is what the contract promises (`x-sdkwork-idempotent: true`, with
/// `Idempotency-Key` required) and what a double-clicked "create" button needs.
#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn repeated_idempotency_key_replays_instead_of_conflicting() {
    let pool = common::postgres_pool().await;
    let repo = repository(pool.clone());
    let request = create_request("Idempotent App", Some("idempotent-app"));

    let first = repo
        .create_app(11, Some(0), Some(1), Some("key-abc"), &request)
        .await
        .expect("first create");
    let before = count_apps(&repo, 11).await;

    let replay = repo
        .create_app(11, Some(0), Some(1), Some("key-abc"), &request)
        .await
        .expect("a replay must succeed, not 409");

    assert_eq!(replay.id, first.id, "replay must return the original app");
    assert_eq!(replay.slug, first.slug);
    assert_eq!(
        count_apps(&repo, 11).await,
        before,
        "a replay must not create a second row"
    );
}

/// Reusing a key with a *different* payload is a client bug and must be
/// rejected — otherwise a retry could silently return the wrong app.
#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn reused_idempotency_key_with_a_different_payload_conflicts() {
    let pool = common::postgres_pool().await;
    let repo = repository(pool.clone());

    repo.create_app(
        13,
        Some(0),
        Some(1),
        Some("key-shared"),
        &create_request("Original App", Some("original-app")),
    )
    .await
    .expect("first create");

    let error = repo
        .create_app(
            13,
            Some(0),
            Some(1),
            Some("key-shared"),
            &create_request("Different App", Some("different-app")),
        )
        .await
        .expect_err("a reused key with a new payload must be rejected");

    assert_eq!(error.kind(), DeployServiceErrorKind::Conflict);
    assert!(
        error.to_string().contains("Idempotency-Key"),
        "the error must name the header: {error}"
    );
}

/// The slug conflict is still reported when the slug really is taken.
///
/// Without this, the idempotency work above could be mistaken for "409 is gone":
/// two *different* apps asking for the same explicit slug must still conflict.
#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn two_apps_with_the_same_explicit_slug_still_conflict() {
    let pool = common::postgres_pool().await;
    let repo = repository(pool.clone());

    repo.create_app(
        15,
        Some(0),
        Some(1),
        None,
        &create_request("First", Some("taken-slug")),
    )
    .await
    .expect("first app claims the slug");

    let error = repo
        .create_app(
            15,
            Some(0),
            Some(1),
            // A *different* key, so this is a genuine duplicate rather than a replay.
            Some("another-key"),
            &create_request("Second", Some("taken-slug")),
        )
        .await
        .expect_err("a genuinely taken slug must still be a conflict");

    assert_eq!(error.kind(), DeployServiceErrorKind::Conflict);
    assert!(error.to_string().contains("taken-slug"), "{error}");
}
