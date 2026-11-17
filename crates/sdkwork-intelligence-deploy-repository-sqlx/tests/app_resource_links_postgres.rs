//! Account bindings against a real PostgreSQL schema: the upsert addressing
//! rules, the payment-default slot, soft delete, and the `0013` backfill from
//! `deploy_app.metadata.integrations`.
//!
//! Requires `SDKWORK_DATABASE_TEST_POSTGRES_URL`; ignored by default like the
//! other PostgreSQL integration tests in this crate (`pnpm test:postgres` runs
//! the ignored set).
//!
//! The harness applies the real lifecycle chain, so this binary also proves that
//! migrations `0012_deploy_application_resource_link_reconcile` and
//! `0013_deploy_application_resource_link_backfill` apply on a schema built from
//! the baseline. The backfill runs there on an empty table (the app does not exist
//! yet), so the migration file is re-executed after the fixture is seeded — which
//! is also what proves its idempotency.

mod common;

use sdkwork_deploy_contract::{
    AppPublishEnvironment, AppResourceKind, AppResourceLinkStatus, DeployServiceErrorKind,
    NormalizedAppResourceLink, UpsertAppResourceLinkRequest,
};
use sdkwork_intelligence_deploy_service::{
    AppResourceLinkRepositoryPort, DeleteAppResourceLinkCommand, ListAppResourceLinksCommand,
    UpsertAppResourceLinkCommand,
};
use serde_json::json;

const TENANT_ID: i64 = 7;
const ORGANIZATION_ID: i64 = 9;
const ACTOR_ID: i64 = 11;

const APP_UUID: &str = "00000000-0000-4000-8000-0000000000b1";
const OTHER_APP_UUID: &str = "00000000-0000-4000-8000-0000000000b2";
const APP_ID: i64 = 9201;
const OTHER_APP_ID: i64 = 9202;

const BACKFILL_SQL: &str = include_str!(
    "../../../database/migrations/postgres/0013_deploy_application_resource_link_backfill.up.sql"
);
const REPORT_SQL: &str = include_str!("../../../tools/report_app_resource_links.sql");

async fn seed_app(pool: &sqlx::PgPool, id: i64, uuid: &str, slug: &str) {
    sqlx::query(
        "INSERT INTO deploy_app (id, uuid, tenant_id, organization_id, name, slug, app_kind,
             app_status, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $5, 'API_SERVICE', 'ACTIVE', NOW(), NOW())",
    )
    .bind(id)
    .bind(uuid)
    .bind(TENANT_ID)
    .bind(0)
    .bind(slug)
    .execute(pool)
    .await
    .expect("insert app");
}

fn binding(
    environment: AppPublishEnvironment,
    resource_kind: AppResourceKind,
    resource_ref: &str,
    provider_code: Option<&str>,
    is_default: bool,
    metadata: serde_json::Value,
) -> NormalizedAppResourceLink {
    UpsertAppResourceLinkRequest {
        environment,
        resource_kind,
        resource_ref: resource_ref.to_owned(),
        provider_code: provider_code.map(str::to_owned),
        is_default,
        binding_status: AppResourceLinkStatus::Active,
        metadata,
    }
    .normalize()
    .expect("the fixture binding is valid")
}

fn upsert(app_uuid: &str, binding: NormalizedAppResourceLink) -> UpsertAppResourceLinkCommand {
    UpsertAppResourceLinkCommand {
        tenant_id: TENANT_ID,
        organization_id: ORGANIZATION_ID,
        actor_id: ACTOR_ID,
        app_uuid: app_uuid.to_owned(),
        binding,
    }
}

fn list_command(app_uuid: &str) -> ListAppResourceLinksCommand {
    ListAppResourceLinksCommand {
        tenant_id: TENANT_ID,
        app_uuid: app_uuid.to_owned(),
        environment: None,
        resource_kind: None,
        page: 1,
        page_size: 20,
    }
}

#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn a_channel_account_is_addressed_by_environment_and_kind() {
    let (repository, pool) = common::migrated_repository_with_pool("app-resource-links-test").await;
    seed_app(&pool, APP_ID, APP_UUID, "resource-link-app").await;

    let official = repository
        .upsert_app_resource_link(upsert(
            APP_UUID,
            binding(
                AppPublishEnvironment::Production,
                AppResourceKind::OfficialAccount,
                "wx-official-1",
                Some("wechat"),
                false,
                json!({ "displayName": "SDKWork official account" }),
            ),
        ))
        .await
        .expect("bind the production official account");
    assert_eq!(official.resource_ref, "wx-official-1");
    assert_eq!(official.environment, AppPublishEnvironment::Production);
    assert_eq!(official.binding_status, AppResourceLinkStatus::Active);
    assert!(
        !official.is_default,
        "a channel account has no default slot"
    );
    assert_eq!(official.version, "1");
    assert_eq!(official.app_id, APP_UUID);

    // Switching the environment's official account replaces the row instead of
    // colliding on `uk_single`: (app, environment, kind) is the address.
    let replaced = repository
        .upsert_app_resource_link(upsert(
            APP_UUID,
            binding(
                AppPublishEnvironment::Production,
                AppResourceKind::OfficialAccount,
                "wx-official-2",
                Some("wechat"),
                false,
                json!({ "displayName": "SDKWork service account" }),
            ),
        ))
        .await
        .expect("replace the production official account");
    assert_eq!(
        replaced.id, official.id,
        "one row per (app, environment, kind): the address is the same binding"
    );
    assert_eq!(replaced.resource_ref, "wx-official-2");
    assert_eq!(replaced.version, "2");

    let page = repository
        .list_app_resource_links(list_command(APP_UUID))
        .await
        .expect("list bindings");
    assert_eq!(page.total, 1, "replacement must not leave a second row");
    assert_eq!(page.items[0].resource_ref, "wx-official-2");
}

#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn the_single_instance_rule_is_per_environment() {
    let (repository, pool) = common::migrated_repository_with_pool("app-resource-links-test").await;
    seed_app(&pool, APP_ID, APP_UUID, "resource-link-app").await;

    for environment in [
        AppPublishEnvironment::Development,
        AppPublishEnvironment::Production,
    ] {
        repository
            .upsert_app_resource_link(upsert(
                APP_UUID,
                binding(
                    environment,
                    AppResourceKind::MiniProgram,
                    "wx-mini-1",
                    Some("wechat"),
                    false,
                    json!({ "displayName": "SDKWork mini program" }),
                ),
            ))
            .await
            .unwrap_or_else(|error| panic!("bind the mini program for {environment:?}: {error}"));
    }

    let page = repository
        .list_app_resource_links(list_command(APP_UUID))
        .await
        .expect("list bindings");
    assert_eq!(
        page.total, 2,
        "the single-instance rule is per (app, environment), so two environments hold two rows"
    );
}

#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn the_payment_default_is_a_slot_per_provider() {
    let (repository, pool) = common::migrated_repository_with_pool("app-resource-links-test").await;
    seed_app(&pool, APP_ID, APP_UUID, "resource-link-app").await;

    let first = repository
        .upsert_app_resource_link(upsert(
            APP_UUID,
            binding(
                AppPublishEnvironment::Production,
                AppResourceKind::PaymentAccount,
                "6222000000000001",
                Some("stripe"),
                true,
                json!({ "accountName": "primary" }),
            ),
        ))
        .await
        .expect("bind the first stripe account as default");
    assert!(first.is_default);

    // Making another stripe account the default *moves* the slot. A blind write
    // would fail on `uk_deploy_application_resource_link_default`.
    let second = repository
        .upsert_app_resource_link(upsert(
            APP_UUID,
            binding(
                AppPublishEnvironment::Production,
                AppResourceKind::PaymentAccount,
                "6222000000000002",
                Some("stripe"),
                true,
                json!({ "accountName": "fallback" }),
            ),
        ))
        .await
        .expect("move the stripe default to the second account");
    assert!(second.is_default);
    assert_ne!(second.id, first.id, "a different reference is another row");

    // A different provider has its own slot.
    repository
        .upsert_app_resource_link(upsert(
            APP_UUID,
            binding(
                AppPublishEnvironment::Production,
                AppResourceKind::PaymentAccount,
                "2022000000000003",
                Some("alipay"),
                true,
                json!({ "accountName": "alipay merchant" }),
            ),
        ))
        .await
        .expect("bind the alipay account as its provider's default");

    let defaults_per_provider: Vec<(String, i64)> = sqlx::query_as(
        "SELECT provider_code, COUNT(*) AS defaults
         FROM deploy_application_resource_link
         WHERE tenant_id = $1 AND app_id = $2 AND resource_kind = 'payment_account'
           AND is_default AND deleted_at IS NULL
         GROUP BY provider_code
         ORDER BY provider_code",
    )
    .bind(TENANT_ID)
    .bind(APP_ID)
    .fetch_all(&pool)
    .await
    .expect("count defaults per provider");
    assert_eq!(
        defaults_per_provider,
        vec![("alipay".to_owned(), 1), ("stripe".to_owned(), 1)],
        "exactly one default per provider, and only the new stripe row may hold it"
    );

    let stripe_default: String = sqlx::query_scalar(
        "SELECT resource_ref FROM deploy_application_resource_link
         WHERE tenant_id = $1 AND app_id = $2 AND provider_code = 'stripe'
           AND is_default AND deleted_at IS NULL",
    )
    .bind(TENANT_ID)
    .bind(APP_ID)
    .fetch_one(&pool)
    .await
    .expect("read the stripe default");
    assert_eq!(stripe_default, "6222000000000002");
}

#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn a_payment_account_is_addressed_by_its_reference() {
    let (repository, pool) = common::migrated_repository_with_pool("app-resource-links-test").await;
    seed_app(&pool, APP_ID, APP_UUID, "resource-link-app").await;

    let first = repository
        .upsert_app_resource_link(upsert(
            APP_UUID,
            binding(
                AppPublishEnvironment::Test,
                AppResourceKind::PaymentAccount,
                "6222000000000009",
                Some("stripe"),
                false,
                json!({ "accountName": "before" }),
            ),
        ))
        .await
        .expect("bind a test-environment merchant account");

    let again = repository
        .upsert_app_resource_link(upsert(
            APP_UUID,
            binding(
                AppPublishEnvironment::Test,
                AppResourceKind::PaymentAccount,
                "6222000000000009",
                Some("stripe"),
                false,
                json!({ "accountName": "after" }),
            ),
        ))
        .await
        .expect("re-bind the same merchant account");
    assert_eq!(again.id, first.id, "same reference, same row");
    assert_eq!(again.metadata["accountName"], json!("after"));
    assert_eq!(again.version, "2");

    let page = repository
        .list_app_resource_links(list_command(APP_UUID))
        .await
        .expect("list bindings");
    assert_eq!(page.total, 1);
}

#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn delete_retires_the_row_without_losing_it() {
    let (repository, pool) = common::migrated_repository_with_pool("app-resource-links-test").await;
    seed_app(&pool, APP_ID, APP_UUID, "resource-link-app").await;
    seed_app(
        &pool,
        OTHER_APP_ID,
        OTHER_APP_UUID,
        "other-resource-link-app",
    )
    .await;

    let bound = repository
        .upsert_app_resource_link(upsert(
            APP_UUID,
            binding(
                AppPublishEnvironment::Staging,
                AppResourceKind::MiniProgram,
                "wx-mini-9",
                Some("wechat"),
                false,
                json!({}),
            ),
        ))
        .await
        .expect("bind a mini program");

    // Another application's binding is not addressable through this application.
    let foreign = repository
        .delete_app_resource_link(DeleteAppResourceLinkCommand {
            tenant_id: TENANT_ID,
            app_uuid: OTHER_APP_UUID.to_owned(),
            resource_link_uuid: bound.id.clone(),
        })
        .await
        .expect_err("a binding of application A must not be deletable through application B");
    assert_eq!(foreign.kind(), DeployServiceErrorKind::NotFound);

    repository
        .delete_app_resource_link(DeleteAppResourceLinkCommand {
            tenant_id: TENANT_ID,
            app_uuid: APP_UUID.to_owned(),
            resource_link_uuid: bound.id.clone(),
        })
        .await
        .expect("retire the binding");

    let page = repository
        .list_app_resource_links(list_command(APP_UUID))
        .await
        .expect("list bindings");
    assert_eq!(page.total, 0, "a retired binding is not listed");

    let still_there: bool = sqlx::query_scalar(
        "SELECT deleted_at IS NOT NULL FROM deploy_application_resource_link
         WHERE tenant_id = $1 AND id = $2",
    )
    .bind(TENANT_ID)
    .bind(&bound.id)
    .fetch_one(&pool)
    .await
    .expect("read the retired row");
    assert!(still_there, "delete is soft: the audit trail stays");

    let second_delete = repository
        .delete_app_resource_link(DeleteAppResourceLinkCommand {
            tenant_id: TENANT_ID,
            app_uuid: APP_UUID.to_owned(),
            resource_link_uuid: bound.id,
        })
        .await
        .expect_err("a retired binding cannot be retired twice");
    assert_eq!(second_delete.kind(), DeployServiceErrorKind::NotFound);
}

#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn a_foreign_tenant_cannot_reach_the_bindings() {
    let (repository, pool) = common::migrated_repository_with_pool("app-resource-links-test").await;
    seed_app(&pool, APP_ID, APP_UUID, "resource-link-app").await;
    repository
        .upsert_app_resource_link(upsert(
            APP_UUID,
            binding(
                AppPublishEnvironment::Production,
                AppResourceKind::OfficialAccount,
                "wx-official-1",
                Some("wechat"),
                false,
                json!({}),
            ),
        ))
        .await
        .expect("bind the official account");

    let mut command = list_command(APP_UUID);
    command.tenant_id = TENANT_ID + 1;
    let error = repository
        .list_app_resource_links(command)
        .await
        .expect_err("another tenant must not resolve this application");
    assert_eq!(error.kind(), DeployServiceErrorKind::NotFound);
}

/// The legacy document, exactly as the console writes it:
/// `deploy_app.metadata.integrations` with one environment and entries that are
/// deliberately both parseable and not.
fn legacy_integrations_document() -> serde_json::Value {
    json!({
        "schema": "sdkwork.application.integrations/v1",
        "linkedApplications": [{ "groupId": "iam", "id": "iam-1" }],
        "environments": {
            "production": {
                "officialAccount": {
                    "id": "iam-official-1",
                    "appId": "wx-official-doc",
                    "displayName": "SDKWork official account",
                    "providerCode": "wechat",
                    "resourceAccountKind": "official-account",
                    "enabled": true
                },
                "miniProgram": {
                    "id": "iam-mini-1",
                    "appId": "wx-mini-doc",
                    "displayName": "SDKWork mini program",
                    "providerCode": "wechat",
                    "resourceAccountKind": "mini-program",
                    "enabled": true
                },
                "paymentAccounts": [
                    {
                        "id": "pay-1",
                        "accountNo": "6222000000000101",
                        "accountName": "primary merchant",
                        "providerCode": "stripe",
                        "environment": "production",
                        "accountMode": "DIRECT",
                        "isDefault": true
                    },
                    {
                        "id": "pay-2",
                        "accountNo": "6222000000000102",
                        "accountName": "second default of the same provider",
                        "providerCode": "stripe",
                        "environment": "production",
                        "accountMode": "DIRECT",
                        "isDefault": true
                    },
                    {
                        "id": "pay-3",
                        "accountNo": "2022000000000103",
                        "accountName": "alipay merchant",
                        "providerCode": "alipay",
                        "environment": "production",
                        "accountMode": "DIRECT",
                        "isDefault": true
                    },
                    {
                        "id": "pay-unparseable",
                        "accountName": "entry without a merchant account",
                        "providerCode": "stripe",
                        "isDefault": false
                    }
                ]
            },
            // Not a lifecycle environment: left in the document, reported by
            // tools/report_app_resource_links.sql.
            "qa": {
                "officialAccount": {
                    "id": "iam-official-qa",
                    "appId": "wx-official-qa",
                    "providerCode": "wechat"
                }
            }
        }
    })
}

async fn count_bindings(pool: &sqlx::PgPool) -> i64 {
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM deploy_application_resource_link
         WHERE tenant_id = $1 AND app_id = $2 AND deleted_at IS NULL",
    )
    .bind(TENANT_ID)
    .bind(APP_ID)
    .fetch_one(pool)
    .await
    .expect("count bindings")
}

#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn the_backfill_normalizes_the_legacy_document_and_repeats_cleanly() {
    let (repository, pool) = common::migrated_repository_with_pool("app-resource-links-test").await;
    seed_app(&pool, APP_ID, APP_UUID, "resource-link-app").await;
    let document = legacy_integrations_document();
    // The console stores the document under the `integrations` key of the app's
    // metadata (`APPLICATION_INTEGRATION_METADATA_KEY`), not as the whole object.
    let metadata = json!({ "integrations": document });
    sqlx::query("UPDATE deploy_app SET metadata = $2 WHERE tenant_id = $1 AND id = $3")
        .bind(TENANT_ID)
        .bind(&metadata)
        .bind(APP_ID)
        .execute(&pool)
        .await
        .expect("write the legacy integrations document");

    sqlx::raw_sql(BACKFILL_SQL)
        .execute(&pool)
        .await
        .expect("the backfill migration must apply");

    // official + mini + three payment rows whose references are present; the
    // entry without `accountNo` and the `qa` environment do not migrate.
    assert_eq!(
        count_bindings(&pool).await,
        5,
        "official, mini, and the three parseable payment accounts"
    );

    let official: (String, Option<String>, bool, String) = sqlx::query_as(
        "SELECT resource_ref, provider_code, is_default, metadata ->> 'sourceAccountId'
         FROM deploy_application_resource_link
         WHERE tenant_id = $1 AND app_id = $2 AND env_key = 'production'
           AND resource_kind = 'official_account' AND deleted_at IS NULL",
    )
    .bind(TENANT_ID)
    .bind(APP_ID)
    .fetch_one(&pool)
    .await
    .expect("read the migrated official account");
    assert_eq!(official.0, "wx-official-doc");
    assert_eq!(official.1.as_deref(), Some("wechat"));
    assert!(!official.2);
    assert_eq!(
        official.3, "iam-official-1",
        "the source account id is kept"
    );

    // Only the first default per provider keeps the slot; the second stripe
    // account is still bound, just not default.
    let stripe_defaults: Vec<(String, bool)> = sqlx::query_as(
        "SELECT resource_ref, is_default FROM deploy_application_resource_link
         WHERE tenant_id = $1 AND app_id = $2 AND provider_code = 'stripe'
           AND deleted_at IS NULL ORDER BY resource_ref",
    )
    .bind(TENANT_ID)
    .bind(APP_ID)
    .fetch_all(&pool)
    .await
    .expect("read the migrated stripe accounts");
    assert_eq!(
        stripe_defaults,
        vec![
            ("6222000000000101".to_owned(), true),
            ("6222000000000102".to_owned(), false),
        ]
    );

    let provenance: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM deploy_application_resource_link
         WHERE tenant_id = $1 AND app_id = $2
           AND metadata ->> 'source' = 'deploy_app.metadata.integrations'",
    )
    .bind(TENANT_ID)
    .bind(APP_ID)
    .fetch_one(&pool)
    .await
    .expect("count rows with provenance");
    assert_eq!(
        provenance, 5,
        "every migrated row records where it came from"
    );

    let ids_before: Vec<String> = sqlx::query_scalar(
        "SELECT id FROM deploy_application_resource_link
         WHERE tenant_id = $1 AND app_id = $2 ORDER BY id",
    )
    .bind(TENANT_ID)
    .bind(APP_ID)
    .fetch_all(&pool)
    .await
    .expect("read the deterministic ids");

    // Re-running the migration must be a no-op: the deterministic id and the
    // natural-key guard make it address the same rows.
    sqlx::raw_sql(BACKFILL_SQL)
        .execute(&pool)
        .await
        .expect("the backfill migration must be repeatable");
    assert_eq!(
        count_bindings(&pool).await,
        5,
        "a second run must not duplicate a binding"
    );
    let ids_after: Vec<String> = sqlx::query_scalar(
        "SELECT id FROM deploy_application_resource_link
         WHERE tenant_id = $1 AND app_id = $2 ORDER BY id",
    )
    .bind(TENANT_ID)
    .bind(APP_ID)
    .fetch_all(&pool)
    .await
    .expect("read the ids after the second run");
    assert_eq!(ids_after, ids_before, "the remap is deterministic");

    // A binding written through the API survives a re-run untouched.
    let created = repository
        .upsert_app_resource_link(upsert(
            APP_UUID,
            binding(
                AppPublishEnvironment::Production,
                AppResourceKind::PaymentAccount,
                "6222000000000999",
                Some("stripe"),
                false,
                json!({ "accountName": "operator bound" }),
            ),
        ))
        .await
        .expect("bind a merchant account through the API");
    sqlx::raw_sql(BACKFILL_SQL)
        .execute(&pool)
        .await
        .expect("a third run is still a no-op");
    let preserved: String = sqlx::query_scalar(
        "SELECT id FROM deploy_application_resource_link
         WHERE tenant_id = $1 AND app_id = $2 AND resource_ref = '6222000000000999'",
    )
    .bind(TENANT_ID)
    .bind(APP_ID)
    .fetch_one(&pool)
    .await
    .expect("read the operator-written binding");
    assert_eq!(preserved, created.id);

    // The source document is evidence, not scratch space: the backfill must not
    // rewrite it.
    let stored_document: serde_json::Value =
        sqlx::query_scalar("SELECT metadata -> 'integrations' FROM deploy_app WHERE id = $1")
            .bind(APP_ID)
            .fetch_one(&pool)
            .await
            .expect("read the source document");
    assert_eq!(stored_document, document);

    // The report must parse and execute against the migrated schema, and it must
    // classify the entry that could not be bound.
    sqlx::raw_sql(REPORT_SQL)
        .execute(&pool)
        .await
        .expect("the report SQL must execute");
    let unmigrated: Vec<(Option<String>, String)> = sqlx::query_as(
        "SELECT binding ->> 'id' AS source_account_id,
                CASE
                    WHEN btrim(coalesce(binding ->> 'accountNo', '')) = '' THEN 'missing_account_no'
                    ELSE 'bound'
                END AS reason
         FROM deploy_app a
         CROSS JOIN LATERAL jsonb_each(a.metadata -> 'integrations' -> 'environments') AS environment(key, value)
         CROSS JOIN LATERAL jsonb_array_elements(
             CASE
                 WHEN jsonb_typeof(environment.value -> 'paymentAccounts') = 'array'
                 THEN environment.value -> 'paymentAccounts'
                 ELSE '[]'::jsonb
             END
         ) AS account(binding)
         WHERE a.id = $1 AND environment.key = 'production'",
    )
    .bind(APP_ID)
    .fetch_all(&pool)
    .await
    .expect("classify the payment entries");
    assert_eq!(
        unmigrated,
        vec![
            (Some("pay-1".to_owned()), "bound".to_owned()),
            (Some("pay-2".to_owned()), "bound".to_owned()),
            (Some("pay-3".to_owned()), "bound".to_owned()),
            (
                Some("pay-unparseable".to_owned()),
                "missing_account_no".to_owned()
            ),
        ],
        "the entry without a merchant account stays in the document and is reported"
    );
}
