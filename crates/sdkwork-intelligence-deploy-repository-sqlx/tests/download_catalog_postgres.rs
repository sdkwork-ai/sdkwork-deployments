//! Download catalog persistence against a real PostgreSQL schema.
//!
//! Skipped without `SDKWORK_DATABASE_TEST_POSTGRES_URL`; run it through
//! `node tools/run-postgres-integration-tests.mjs --package
//! sdkwork-intelligence-deploy-repository-sqlx`.

mod common;

use sdkwork_database_id::SnowflakeIdGenerator;
use sdkwork_deploy_contract::{
    UpsertDownloadCardRequest, UpsertDownloadCatalogRequest, UpsertDownloadItemRequest,
    UpsertDownloadUpdateRequest, DOWNLOAD_CATALOG_STATUS_PUBLISHED,
};
use sdkwork_intelligence_deploy_repository_sqlx::DeployRepository;
use sdkwork_intelligence_deploy_service::DeployRepositoryPort;

const TENANT: i64 = 91_700_001;

async fn repository() -> DeployRepository {
    let pool = common::postgres_pool().await;
    DeployRepository::new(
        pool,
        SnowflakeIdGenerator::new(5).expect("Snowflake generator"),
        common::test_secret_key(),
    )
}

#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn download_catalog_round_trip_and_publication() {
    let repository = repository().await;
    repository
        .upsert_download_catalog(
            TENANT,
            None,
            "demo-catalog",
            &UpsertDownloadCatalogRequest {
                product_key: Some("app.demo".to_owned()),
                product_name: Some("Demo".to_owned()),
                product_version: Some("1.0.0".to_owned()),
                default_channel_key: Some("nightly".to_owned()),
                app_id: None,
            },
        )
        .await
        .expect("catalog");
    repository
        .upsert_download_card(
            TENANT,
            None,
            "demo-catalog",
            "desktop",
            &UpsertDownloadCardRequest {
                title: Some("Desktop".to_owned()),
                description: Some("Installers".to_owned()),
                ..Default::default()
            },
        )
        .await
        .expect("card");
    repository
        .upsert_download_item(
            TENANT,
            None,
            "demo-catalog",
            "desktop",
            "win-x64",
            &UpsertDownloadItemRequest {
                platform: Some("windows".to_owned()),
                architecture: Some("x64".to_owned()),
                label: Some("Windows".to_owned()),
                version: Some("1.0.0".to_owned()),
                download_source: Some(serde_json::json!({
                    "href": "https://download.example.com/demo.exe"
                })),
                size_bytes: Some(1024),
                checksum_sha256: Some("abc".to_owned()),
                primary_action: Some(true),
                ..Default::default()
            },
        )
        .await
        .expect("installer");
    repository
        .upsert_download_update(
            TENANT,
            None,
            "demo-catalog",
            "desktop",
            "win-x64",
            &UpsertDownloadUpdateRequest {
                enabled: Some(true),
                provider: Some("generic".to_owned()),
                feed_url: Some("https://download.example.com/feed/".to_owned()),
                channel_key: Some("nightly".to_owned()),
                allowed_channels: Some(vec!["nightly".to_owned()]),
                rollout_percentage: Some(25),
                ..Default::default()
            },
        )
        .await
        .expect("update config");

    let draft = repository
        .retrieve_download_catalog(Some(TENANT), "demo-catalog")
        .await
        .expect("draft read");
    assert_ne!(draft.status, DOWNLOAD_CATALOG_STATUS_PUBLISHED);
    assert_eq!(draft.cards.len(), 1);
    assert_eq!(draft.cards[0].items.len(), 1);
    let update = draft.cards[0].items[0]
        .update
        .as_ref()
        .expect("update configuration");
    assert_eq!(update.provider, "generic");
    assert_eq!(update.rollout_percentage, 25);

    // A draft is invisible to the public surface.
    assert!(repository
        .published_download_catalogs("demo-catalog")
        .await
        .expect("public draft read")
        .is_empty());
    assert!(repository
        .published_download_record("app.demo", "windows", "x64")
        .await
        .expect("public record before publish")
        .is_none());

    let published = repository
        .publish_download_catalog(TENANT, None, "demo-catalog")
        .await
        .expect("publish");
    assert_eq!(published.status, DOWNLOAD_CATALOG_STATUS_PUBLISHED);
    assert_eq!(published.revision, 2);
    assert!(published.published_at.is_some());

    let public = repository
        .published_download_catalogs("demo-catalog")
        .await
        .expect("public catalog");
    assert_eq!(public.len(), 1);
    assert_eq!(public[0].cards.len(), 1);
    assert_eq!(public[0].cards[0].items.len(), 1);

    let record = repository
        .published_download_record("app.demo", "windows", "x64")
        .await
        .expect("public record")
        .expect("published installer");
    assert_eq!(record.item.item_key, "win-x64");
    assert_eq!(record.catalog.catalog_key, "demo-catalog");

    // Soft-deleting the card removes it from both operator and public reads.
    repository
        .delete_download_card(TENANT, "demo-catalog", "desktop")
        .await
        .expect("delete card");
    let after = repository
        .retrieve_download_catalog(Some(TENANT), "demo-catalog")
        .await
        .expect("read after delete");
    assert!(after.cards.is_empty());
    assert!(repository
        .published_download_record("app.demo", "windows", "x64")
        .await
        .expect("public record after delete")
        .is_none());
}

#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn catalog_upsert_is_idempotent() {
    let repository = repository().await;
    let request = UpsertDownloadCatalogRequest {
        product_key: Some("app.demo".to_owned()),
        product_name: Some("Demo".to_owned()),
        product_version: Some("1.0.0".to_owned()),
        default_channel_key: Some("nightly".to_owned()),
        app_id: None,
    };
    let first = repository
        .upsert_download_catalog(TENANT, None, "repeat-catalog", &request)
        .await
        .expect("first upsert");
    let second = repository
        .upsert_download_catalog(TENANT, None, "repeat-catalog", &request)
        .await
        .expect("second upsert");
    assert_eq!(first.id, second.id);
    assert_eq!(second.catalog_key, "repeat-catalog");
    assert!(second.version.parse::<i64>().unwrap() > first.version.parse::<i64>().unwrap());
}
