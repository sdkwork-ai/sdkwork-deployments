//! Factory reconciliation (`APPLICATION_FACTORY_DEPLOYMENT_SPEC.md` section 9).
//!
//! The reconciliation read model joins tables owned by sibling modules, so its
//! correctness is a database fact: which cross-module links are dangling, and
//! whether a schema that does not host the sibling authority degrades to
//! `unavailable` instead of failing. These tests create the minimal sibling
//! tables in the test schema (the deploy lifecycle does not migrate them) and
//! assert the closed finding codes.
//!
//! Requires `SDKWORK_DATABASE_TEST_POSTGRES_URL`; ignored by default like the
//! other PostgreSQL integration tests in this crate.

mod common;

use sdkwork_deploy_contract::{AppKind, CreateAppRequest};
use sdkwork_intelligence_deploy_service::DeployRepositoryPort;
use sqlx::PgPool;

const TENANT: i64 = 7201;
const OTHER_TENANT: i64 = 7202;

fn app_request(name: &str, slug: &str) -> CreateAppRequest {
    serde_json::from_value(serde_json::json!({
        "name": name,
        "slug": slug,
        "appKind": AppKind::PcWeb.as_str(),
    }))
    .expect("CreateAppRequest parses from its own wire shape")
}

/// Minimal sibling tables: only the columns the reconciliation reads.
async fn create_sibling_tables(pool: &PgPool) {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS iam_application_installation (
             id TEXT PRIMARY KEY,
             tenant_id TEXT NOT NULL,
             deploy_application_id TEXT,
             status TEXT NOT NULL,
             provisioning_status TEXT NOT NULL DEFAULT 'pending'
         )",
    )
    .execute(pool)
    .await
    .expect("create iam_application_installation stub");

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS iam_application_installation_environment (
             id TEXT PRIMARY KEY,
             installation_id TEXT NOT NULL,
             env_key TEXT NOT NULL,
             primary_domain TEXT,
             env_status TEXT NOT NULL DEFAULT 'active',
             deleted_at TIMESTAMPTZ
         )",
    )
    .execute(pool)
    .await
    .expect("create iam environment stub");

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS webserver_application (
             uuid TEXT PRIMARY KEY,
             tenant_id BIGINT NOT NULL,
             deploy_application_id BIGINT,
             deleted_at TIMESTAMPTZ
         )",
    )
    .execute(pool)
    .await
    .expect("create webserver_application stub");
}

#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn a_clean_factory_reconciles_without_findings() {
    let (repository, pool) =
        common::migrated_repository_with_pool("sdkwork-deploy-reconcile-clean-test").await;
    create_sibling_tables(&pool).await;

    let app = repository
        .create_app(
            TENANT,
            None,
            Some(1),
            None,
            &app_request("Clean App", "clean-app"),
        )
        .await
        .expect("create an application");
    sqlx::query("UPDATE deploy_app SET identity_status = 'active' WHERE uuid = $1")
        .bind(&app.id)
        .execute(&pool)
        .await
        .expect("mark provisioned");
    sqlx::query(
        "INSERT INTO iam_application_installation
           (id, tenant_id, deploy_application_id, status, provisioning_status)
         VALUES ('inst_1', $1, $2, 'enabled', 'provisioned')",
    )
    .bind(TENANT.to_string())
    .bind(app.deploy_application_id.to_string())
    .execute(&pool)
    .await
    .expect("insert an installation");

    let report = repository
        .reconcile_factory_references(TENANT)
        .await
        .expect("reconcile");
    assert_eq!(report.status, "CLEAN", "{report:?}");
    assert!(report.dangling.is_empty(), "{report:?}");
    assert_eq!(report.apps_checked, "1");
}

#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn dangling_identity_carrier_and_domain_references_are_named() {
    let (repository, pool) =
        common::migrated_repository_with_pool("sdkwork-deploy-reconcile-drift-test").await;
    create_sibling_tables(&pool).await;

    // 1. Identity active without an installation.
    let orphan_identity = repository
        .create_app(
            TENANT,
            None,
            Some(1),
            None,
            &app_request("Orphan Identity", "orphan-identity"),
        )
        .await
        .expect("create app");
    sqlx::query("UPDATE deploy_app SET identity_status = 'active' WHERE uuid = $1")
        .bind(&orphan_identity.id)
        .execute(&pool)
        .await
        .expect("mark provisioned");

    // 2. Completed uninstall whose installation is still enabled.
    let uninstalled = repository
        .create_app(
            TENANT,
            None,
            Some(1),
            None,
            &app_request("Uninstalled App", "uninstalled-app"),
        )
        .await
        .expect("create app");
    sqlx::query(
        "UPDATE deploy_app SET app_status = 'ARCHIVED', uninstall_status = 'complete' WHERE uuid = $1",
    )
    .bind(&uninstalled.id)
    .execute(&pool)
    .await
    .expect("mark uninstalled");
    sqlx::query(
        "INSERT INTO iam_application_installation
           (id, tenant_id, deploy_application_id, status, provisioning_status)
         VALUES ('inst_still_enabled', $1, $2, 'enabled', 'provisioned')",
    )
    .bind(TENANT.to_string())
    .bind(uninstalled.deploy_application_id.to_string())
    .execute(&pool)
    .await
    .expect("insert enabled installation");

    // 3. An installation environment domain the deploy plan never published.
    let unplanned = repository
        .create_app(
            TENANT,
            None,
            Some(1),
            None,
            &app_request("Unplanned Domain", "unplanned-domain"),
        )
        .await
        .expect("create app");
    sqlx::query("UPDATE deploy_app SET identity_status = 'active' WHERE uuid = $1")
        .bind(&unplanned.id)
        .execute(&pool)
        .await
        .expect("mark provisioned");
    sqlx::query(
        "INSERT INTO iam_application_installation
           (id, tenant_id, deploy_application_id, status, provisioning_status)
         VALUES ('inst_unplanned', $1, $2, 'enabled', 'provisioned')",
    )
    .bind(TENANT.to_string())
    .bind(unplanned.deploy_application_id.to_string())
    .execute(&pool)
    .await
    .expect("insert installation");
    sqlx::query(
        "INSERT INTO iam_application_installation_environment
           (id, installation_id, env_key, primary_domain, env_status)
         VALUES ('env_unplanned', 'inst_unplanned', 'production', 'nobody-planned-this.example', 'active')",
    )
    .execute(&pool)
    .await
    .expect("insert installation environment");

    // 4. A carrier pointing at an anchor no deploy application owns.
    sqlx::query(
        "INSERT INTO webserver_application (uuid, tenant_id, deploy_application_id)
         VALUES ('carrier-orphan', $1, 999999)",
    )
    .bind(TENANT)
    .execute(&pool)
    .await
    .expect("insert orphan carrier");

    // A second live carrier for one application violates the 1:1 rule.
    sqlx::query(
        "INSERT INTO webserver_application (uuid, tenant_id, deploy_application_id)
         VALUES ('carrier-a', $1, $2), ('carrier-b', $1, $2)",
    )
    .bind(TENANT)
    .bind(orphan_identity.deploy_application_id)
    .execute(&pool)
    .await
    .expect("insert duplicate carriers");

    let report = repository
        .reconcile_factory_references(TENANT)
        .await
        .expect("reconcile");
    assert_eq!(report.status, "DRIFT", "{report:?}");
    let codes: Vec<&str> = report.dangling.iter().map(|d| d.code.as_str()).collect();
    assert!(
        codes.contains(&"IDENTITY_ACTIVE_WITHOUT_INSTALLATION"),
        "{codes:?}"
    );
    assert!(
        codes.contains(&"UNINSTALLED_INSTALLATION_STILL_ENABLED"),
        "{codes:?}"
    );
    assert!(
        codes.contains(&"INSTALLATION_DOMAIN_NOT_PLANNED"),
        "{codes:?}"
    );
    assert!(
        codes.contains(&"MULTIPLE_CARRIERS_FOR_ONE_APPLICATION"),
        "{codes:?}"
    );
    assert!(
        codes.contains(&"CARRIER_WITHOUT_DEPLOY_APPLICATION"),
        "{codes:?}"
    );

    // Another tenant's applications are not inspected: the report is
    // tenant-scoped, not a platform-wide scan.
    let other = repository
        .reconcile_factory_references(OTHER_TENANT)
        .await
        .expect("reconcile another tenant");
    assert_eq!(other.apps_checked, "0");
    assert!(other.dangling.is_empty(), "{other:?}");
}

#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn a_schema_without_the_sibling_authorities_degrades_to_partial() {
    let (repository, _pool) =
        common::migrated_repository_with_pool("sdkwork-deploy-reconcile-partial-test").await;
    // No IAM / webserver stubs: the deploy lifecycle only migrates deploy tables.
    let report = repository
        .reconcile_factory_references(TENANT)
        .await
        .expect("reconcile a deploy-only schema");
    assert_eq!(report.status, "PARTIAL", "{report:?}");
    assert!(report.dangling.is_empty(), "{report:?}");
    assert!(report
        .unavailable
        .iter()
        .any(|entry| entry.starts_with("iam")));
    assert!(report
        .unavailable
        .iter()
        .any(|entry| entry.starts_with("webserver")));
}
