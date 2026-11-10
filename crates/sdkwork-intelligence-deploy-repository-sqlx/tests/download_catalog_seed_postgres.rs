//! BirdCoder download-catalog seeds against a real PostgreSQL schema.
//!
//! The seed manifest is the production path; these tests execute the same SQL
//! files the lifecycle would run for the `test` and `production` profiles and
//! assert the rows an operator would otherwise type by hand: six installers
//! across the supported platform/architecture pairs, a published catalog, and
//! per-profile channel/rollout values.
//!
//! Skipped without `SDKWORK_DATABASE_TEST_POSTGRES_URL`.

mod common;

use sqlx::{AssertSqlSafe, PgPool};

const CORE: &str = include_str!("../../../database/seeds/common/010_birdcoder_catalog_core.sql");
const TEST_PROFILE: &str =
    include_str!("../../../database/seeds/common/011_birdcoder_catalog_test.sql");
const PRODUCTION_PROFILE: &str =
    include_str!("../../../database/seeds/common/014_birdcoder_catalog_production.sql");

async fn apply_seed(pool: &PgPool, script: &str) {
    sqlx::raw_sql(AssertSqlSafe(script.to_owned()))
        .execute(pool)
        .await
        .expect("apply seed script");
}

#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn test_profile_seeds_six_installers_and_nightly_configuration() {
    let pool = common::postgres_pool().await;
    apply_seed(&pool, CORE).await;
    apply_seed(&pool, TEST_PROFILE).await;

    let status: String =
        sqlx::query_scalar("SELECT status FROM deploy_download_catalog WHERE id = 910001200001")
            .fetch_one(&pool)
            .await
            .expect("catalog");
    assert_eq!(status, "PUBLISHED");

    let version: String = sqlx::query_scalar(
        "SELECT product_version FROM deploy_download_catalog WHERE id = 910001200001",
    )
    .fetch_one(&pool)
    .await
    .expect("version");
    assert_eq!(version, "0.2.2-rc.1");

    let installers: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM deploy_download_item WHERE deleted_at IS NULL")
            .fetch_one(&pool)
            .await
            .expect("installer count");
    assert_eq!(installers, 6);

    for (platform, architecture) in [
        ("windows", "x64"),
        ("windows", "arm64"),
        ("macos", "arm64"),
        ("macos", "x64"),
        ("linux", "x64"),
        ("linux", "arm64"),
    ] {
        let href: String = sqlx::query_scalar(
            "SELECT download_source->>'href' FROM deploy_download_item
             WHERE platform = $1 AND architecture = $2 AND deleted_at IS NULL",
        )
        .bind(platform)
        .bind(architecture)
        .fetch_one(&pool)
        .await
        .expect("installer href");
        assert!(
            href.starts_with(
                "https://www.modelscope.cn/datasets/sdkwork/birdcoder/resolve/master/"
            ),
            "unexpected href for {platform}/{architecture}: {href}"
        );
    }

    let (channel, interval, rollout, enabled): (String, i32, i32, bool) = sqlx::query_as(
        "SELECT channel_key, check_interval_minutes, rollout_percentage, enabled
         FROM deploy_download_update ORDER BY id LIMIT 1",
    )
    .fetch_one(&pool)
    .await
    .expect("update configuration");
    assert_eq!(channel, "nightly");
    assert_eq!(interval, 10);
    assert_eq!(rollout, 100);
    // The release pipeline must upload the channel file before checks are safe.
    assert!(!enabled);
}

#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn production_profile_seeds_stable_channel_and_staged_rollout() {
    let pool = common::postgres_pool().await;
    apply_seed(&pool, CORE).await;
    apply_seed(&pool, PRODUCTION_PROFILE).await;

    let (version, channel): (String, String) = sqlx::query_as(
        "SELECT product_version, default_channel_key FROM deploy_download_catalog WHERE id = 910001200001",
    )
    .fetch_one(&pool)
    .await
    .expect("catalog");
    assert_eq!(version, "0.2.2");
    assert_eq!(channel, "stable");

    let (interval, rollout): (i32, i32) = sqlx::query_as(
        "SELECT check_interval_minutes, rollout_percentage FROM deploy_download_update ORDER BY id LIMIT 1",
    )
    .fetch_one(&pool)
    .await
    .expect("update configuration");
    assert_eq!(interval, 30);
    assert_eq!(rollout, 20);
}

#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn reseeding_the_same_profile_stays_idempotent() {
    let pool = common::postgres_pool().await;
    apply_seed(&pool, CORE).await;
    apply_seed(&pool, TEST_PROFILE).await;
    apply_seed(&pool, CORE).await;
    apply_seed(&pool, TEST_PROFILE).await;

    let installers: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM deploy_download_item")
        .fetch_one(&pool)
        .await
        .expect("installer count");
    assert_eq!(installers, 6);
    let updates: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM deploy_download_update")
        .fetch_one(&pool)
        .await
        .expect("update count");
    assert_eq!(updates, 6);
}
