//! Schema proof for migration 0017 (`channel_release`).
//!
//! Applies the full forward migration chain to an isolated schema, then asserts
//! the channel tables, the resource-link access-mode columns, and the dedupe
//! indexes exist. This is the only place the migration SQL itself is executed.
//!
//! Requires `SDKWORK_DATABASE_TEST_POSTGRES_URL`.
mod common;

use sqlx::{PgPool, Row};
use std::collections::HashSet;

async fn columns_of(pool: &PgPool, table: &str) -> HashSet<String> {
    sqlx::query(
        "SELECT column_name FROM information_schema.columns \
         WHERE table_schema = current_schema() AND table_name = $1",
    )
    .bind(table)
    .fetch_all(pool)
    .await
    .expect("read columns")
    .into_iter()
    .map(|row| row.get::<String, _>("column_name"))
    .collect()
}

async fn indexes_of(pool: &PgPool, table: &str) -> HashSet<String> {
    sqlx::query(
        "SELECT indexname FROM pg_indexes \
         WHERE schemaname = current_schema() AND tablename = $1",
    )
    .bind(table)
    .fetch_all(pool)
    .await
    .expect("read indexes")
    .into_iter()
    .map(|row| row.get::<String, _>("indexname"))
    .collect()
}

#[tokio::test]
async fn channel_release_schema_is_applied() {
    let (_repository, pool) = common::migrated_repository_with_pool("channel-release-schema").await;

    for table in [
        "deploy_channel_release",
        "deploy_channel_template",
        "deploy_channel_domain",
    ] {
        let present: bool = sqlx::query_scalar("SELECT to_regclass($1) IS NOT NULL")
            .bind(table)
            .fetch_one(&pool)
            .await
            .expect("read regclass");
        assert!(present, "{table} is missing");
    }

    let link_columns = columns_of(&pool, "deploy_application_resource_link").await;
    for column in [
        "access_mode",
        "channel_account_id",
        "operator_platform_id",
        "authorization_ref",
        "capability_snapshot_json",
    ] {
        assert!(
            link_columns.contains(column),
            "deploy_application_resource_link is missing {column}"
        );
    }

    let release_columns = columns_of(&pool, "deploy_channel_release").await;
    for column in [
        "channel",
        "access_mode",
        "provider_version",
        "template_id",
        "commit_id",
        "audit_id",
        "release_status",
        "gray_percentage",
        "rollback_of_release_id",
        "provider_payload_json",
        "submitted_at",
        "released_at",
    ] {
        assert!(
            release_columns.contains(column),
            "deploy_channel_release is missing {column}"
        );
    }

    let release_indexes = indexes_of(&pool, "deploy_channel_release").await;
    for index in [
        "idx_deploy_channel_release_app",
        "uk_deploy_channel_release_commit",
    ] {
        assert!(
            release_indexes.contains(index),
            "deploy_channel_release is missing index {index}"
        );
    }

    let domain_indexes = indexes_of(&pool, "deploy_channel_domain").await;
    assert!(
        domain_indexes.contains("uk_deploy_channel_domain_value"),
        "deploy_channel_domain is missing the domain uniqueness index"
    );

    let constraint: bool =
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = $1)")
            .bind("chk_deploy_application_resource_link_access_mode")
            .fetch_one(&pool)
            .await
            .expect("read constraint");
    assert!(constraint, "resource link access-mode check is missing");
}
