//! App template marketplace catalog integration test against a real
//! PostgreSQL: category administration, author listing create/update, review
//! approval, marketplace browse facets, and the tenant boundary.
//!
//! The module is catalog-only. Acquisition, payment and entitlement are owned
//! by the sdkwork-order order center and sdkwork-payment, so this test never
//! acquires, lists an inventory or revokes anything.
//!
//! Runs only with `SDKWORK_DATABASE_TEST_POSTGRES_URL` set; the schema is the
//! disposable `postgres_pool()` baseline (see tests/common/mod.rs).

mod common;

use sdkwork_database_id::SnowflakeIdGenerator;
use sdkwork_deploy_contract::{
    CreateAppTemplateRequest, CreateAppTemplateVersionRequest, CreateTemplateCategoryRequest,
    DeployAppApi, DeployAppRequestContext, DeployBackendApi, DeployBackendRequestContext,
    ListMarketplaceTemplatesQuery, UpdateAppTemplateAdminRequest, UpdateAppTemplateRequest,
};
use sdkwork_intelligence_deploy_repository_sqlx::DeployRepository;
use sdkwork_intelligence_deploy_service::{DeployRepositoryPort, DeployService};
use std::sync::Arc;

fn author(actor: i64) -> DeployAppRequestContext {
    DeployAppRequestContext {
        tenant_id: 7,
        actor_id: Some(actor),
        organization_id: Some(0),
        ..DeployAppRequestContext::default()
    }
}

fn viewer(actor: i64) -> DeployAppRequestContext {
    author(actor)
}

fn operator() -> DeployBackendRequestContext {
    DeployBackendRequestContext {
        operator_id: Some(99),
        tenant_id: Some(7),
    }
}

fn category_request(key: &str) -> CreateTemplateCategoryRequest {
    CreateTemplateCategoryRequest {
        parent_id: None,
        category_key: key.to_owned(),
        display_name: format!("Category {key}"),
        description: None,
        sort_order: Some(0),
    }
}

fn template_request(
    key: &str,
    app_uuid: &str,
    category_uuid: &str,
    pricing: &str,
) -> CreateAppTemplateRequest {
    CreateAppTemplateRequest {
        app_uuid: app_uuid.to_owned(),
        template_type: None,
        category_uuid: category_uuid.to_owned(),
        template_key: key.to_owned(),
        display_name: format!("Template {key}"),
        summary: "A catalog listing".to_owned(),
        description: None,
        visibility: Some("PUBLIC".to_owned()),
        pricing_model: Some(pricing.to_owned()),
        price_minor: if pricing == "PAID" {
            Some("1200".to_owned())
        } else {
            None
        },
        currency: None,
        icon_media_ref: None,
        cover_media_ref: None,
        initial_version: Some(CreateAppTemplateVersionRequest {
            version: "1.0.0".to_owned(),
            changelog: None,
            artifact_uuid: None,
            source_app_version: None,
            platform_targets: None,
            package_size_bytes: None,
            checksum_sha256: None,
        }),
    }
}

#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn postgres_marketplace_catalog_flow_publishes_and_stays_tenant_bounded() {
    let pool = common::postgres_pool().await;
    let repository = Arc::new(DeployRepository::new(
        pool.clone(),
        SnowflakeIdGenerator::new(4).expect("Snowflake generator"),
        common::test_secret_key(),
    ));
    let service = Arc::new(DeployService::new(
        repository.clone() as Arc<dyn DeployRepositoryPort>,
        Arc::new(sdkwork_deploy_drive_port::MemoryDeployDrivePort::default()),
    ));
    let suffix = sdkwork_database_id::uuid_v4().replace('-', "");
    let author_ctx = author(11);
    let viewer_ctx = viewer(12);

    // 1. Admin category CRUD, then the author-visible category tree.
    let category = service
        .create_template_category(&operator(), &category_request(&format!("cat{suffix}")))
        .await
        .expect("create category");
    assert_eq!(category.status, "ACTIVE");
    let categories = DeployAppApi::list_template_categories(&*service, &author_ctx, false)
        .await
        .expect("list categories");
    assert!(categories.items.iter().any(|item| item.id == category.id));

    // 2. Author publishes a FREE template over an owned app. The app must
    //    exist first: resolve_app_internal_id refuses foreign or deleted rows.
    let app = service
        .create_app(
            &author_ctx,
            None,
            &sdkwork_deploy_contract::CreateAppRequest {
                name: format!("app{suffix}"),
                slug: Some(format!("app{suffix}")),
                app_kind: sdkwork_deploy_contract::AppKind::ApiService,
                app_type: Some(2),
                runtime_config: None,
                metadata: None,
                description: None,
                default_environment: None,
                app_domain_label: None,
                app_domain_suffixes: None,
                owner_type: None,
                source_specs: None,
                idempotency_key: None,
            },
        )
        .await
        .expect("create source app");
    let free = service
        .create_app_template(
            &author_ctx,
            "idem-free",
            &template_request(&format!("free{suffix}"), &app.id, &category.id, "FREE"),
        )
        .await
        .expect("create free template");
    assert_eq!(free.status, "DRAFT");
    assert_eq!(free.template_type, "APP");
    assert_eq!(
        free.install_count, "0",
        "the commerce-fed install counter starts at zero and is not module-written"
    );

    // 3. The author edits the draft listing; the catalog keeps the new copy and
    //    stays in DRAFT until review.
    let edited = DeployAppApi::update_app_template(
        &*service,
        &author_ctx,
        &free.id,
        &UpdateAppTemplateRequest {
            display_name: Some(format!("Template free{suffix} v2")),
            summary: Some("An edited catalog listing".to_owned()),
            ..UpdateAppTemplateRequest::default()
        },
    )
    .await
    .expect("edit draft template");
    assert_eq!(edited.display_name, format!("Template free{suffix} v2"));
    assert_eq!(edited.status, "DRAFT");

    // 4. Submit refuses nothing less than a version; approval publishes both.
    let submitted = DeployAppApi::submit_app_template(&*service, &author_ctx, &free.id)
        .await
        .expect("submit for review");
    assert_eq!(submitted.status, "PENDING_REVIEW");
    let published = DeployBackendApi::update_app_template(
        &*service,
        &operator(),
        &free.id,
        &UpdateAppTemplateAdminRequest {
            status: Some("PUBLISHED".to_owned()),
            review_note: Some("ok".to_owned()),
            is_featured: None,
            visibility: None,
        },
    )
    .await
    .expect("approve template");
    assert_eq!(published.status, "PUBLISHED");
    assert!(
        published.latest_version_uuid.is_some(),
        "approval pins the newest published version"
    );

    // 5. Marketplace browse facets: pricing + type filters; a PUBLISHED PAID
    //    listing stays catalogue-visible while only PUBLISHED ones appear.
    let paid = service
        .create_app_template(
            &author(13),
            "idem-paid",
            &template_request(&format!("paid{suffix}"), &app.id, &category.id, "PAID"),
        )
        .await
        .expect("create paid template");
    let versions =
        DeployAppApi::list_app_template_versions(&*service, &author(13), &paid.id, 1, 20)
            .await
            .expect("versions of draft template");
    assert_eq!(versions.items.len(), 1);
    let paid_approved = DeployBackendApi::update_app_template(
        &*service,
        &operator(),
        &paid.id,
        &UpdateAppTemplateAdminRequest {
            status: Some("PUBLISHED".to_owned()),
            review_note: None,
            is_featured: None,
            visibility: None,
        },
    )
    .await
    .expect("approve paid template");
    assert!(paid_approved.latest_version_uuid.is_some());

    let browsed = DeployAppApi::list_marketplace_templates(
        &*service,
        &viewer_ctx,
        &ListMarketplaceTemplatesQuery {
            page: 1,
            page_size: 20,
            keyword: None,
            category_uuid: None,
            pricing_model: Some("FREE".to_owned()),
            template_type: Some("APP".to_owned()),
            sort: None,
        },
    )
    .await
    .expect("browse marketplace");
    assert!(browsed.items.iter().any(|item| item.id == free.id));
    assert!(!browsed.items.iter().any(|item| item.id == paid.id));

    let popular = DeployAppApi::list_marketplace_templates(
        &*service,
        &viewer_ctx,
        &ListMarketplaceTemplatesQuery {
            sort: Some("POPULAR".to_owned()),
            ..ListMarketplaceTemplatesQuery::default()
        },
    )
    .await
    .expect("browse by install count");
    assert!(
        popular.items.iter().any(|item| item.id == free.id),
        "install-count ranking reads the commerce-fed counter"
    );

    let typed = DeployAppApi::list_marketplace_templates(
        &*service,
        &viewer_ctx,
        &ListMarketplaceTemplatesQuery {
            template_type: Some("VIDEO".to_owned()),
            ..ListMarketplaceTemplatesQuery::default()
        },
    )
    .await
    .expect("browse by type");
    assert!(typed.items.is_empty(), "no VIDEO templates exist yet");

    // 6. Tenant boundary: the same listing is invisible to another tenant's
    //    marketplace browse and to a direct catalog read.
    let outsider = DeployAppRequestContext {
        tenant_id: 8,
        actor_id: Some(21),
        organization_id: Some(0),
        ..DeployAppRequestContext::default()
    };
    let foreign_browse = DeployAppApi::list_marketplace_templates(
        &*service,
        &outsider,
        &ListMarketplaceTemplatesQuery::default(),
    )
    .await
    .expect("foreign browse");
    assert!(foreign_browse.items.is_empty());
    let foreign_read =
        DeployAppApi::retrieve_marketplace_template(&*service, &outsider, &free.id).await;
    assert!(
        foreign_read.is_err(),
        "cross-tenant catalog read must be refused"
    );
}
