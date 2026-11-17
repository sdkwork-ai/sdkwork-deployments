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
    AppTemplateScreenshot, CreateAppTemplateRequest, CreateAppTemplateVersionRequest,
    CreateTemplateCategoryRequest, DeployAppApi, DeployAppRequestContext, DeployBackendApi,
    DeployBackendRequestContext, DeployServiceError, ListMarketplaceTemplatesQuery,
    UpdateAppTemplateAdminRequest, UpdateAppTemplateRequest,
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
        screenshots: Vec::new(),
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
            description: Some("The long listing description".to_owned()),
            icon_media_ref: Some("drive://spaces/s/nodes/icon".to_owned()),
            cover_media_ref: Some("drive://spaces/s/nodes/cover".to_owned()),
            screenshots: Some(vec![
                AppTemplateScreenshot {
                    media_ref: "drive://spaces/s/nodes/page-1".to_owned(),
                    device: Some("iphone-69".to_owned()),
                    caption: Some("Overview".to_owned()),
                    sort_order: 0,
                },
                AppTemplateScreenshot {
                    media_ref: "drive://spaces/s/nodes/page-2".to_owned(),
                    device: Some("android-phone".to_owned()),
                    caption: None,
                    sort_order: 1,
                },
            ]),
            ..UpdateAppTemplateRequest::default()
        },
    )
    .await
    .expect("edit draft template");
    assert_eq!(edited.display_name, format!("Template free{suffix} v2"));
    assert_eq!(edited.description, "The long listing description");
    assert_eq!(
        edited.icon_media_ref.as_deref(),
        Some("drive://spaces/s/nodes/icon")
    );
    assert_eq!(
        edited.cover_media_ref.as_deref(),
        Some("drive://spaces/s/nodes/cover")
    );
    assert_eq!(edited.screenshots.len(), 2);
    assert_eq!(edited.screenshots[0].caption.as_deref(), Some("Overview"));
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
            page: 1,
            page_size: 20,
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
            page: 1,
            page_size: 20,
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

    // 7. Install by a consumer: a different user of the same tenant reads
    //    another author's published listing and its published version through
    //    the marketplace view — the author scope no longer blanks it.
    let consumer_versions =
        DeployAppApi::list_app_template_versions(&*service, &viewer_ctx, &free.id, 1, 20)
            .await
            .expect("consumer reads the published versions of a market template");
    assert_eq!(consumer_versions.items.len(), 1);
    assert_eq!(consumer_versions.items[0].status, "PUBLISHED");
    assert!(
        consumer_versions
            .items
            .iter()
            .all(|version| version.status == "PUBLISHED"),
        "a consumer never sees a draft version"
    );
    let consumer_listing = DeployAppApi::retrieve_app_template(&*service, &viewer_ctx, &free.id)
        .await
        .expect("consumer reads the published listing");
    assert_eq!(consumer_listing.id, free.id);
    assert_eq!(consumer_listing.author_user_id, "11");

    // 8. The author keeps the author-scope read on their own unpublished work.
    let draft = service
        .create_app_template(
            &author_ctx,
            "idem-draft",
            &template_request(&format!("draft{suffix}"), &app.id, &category.id, "FREE"),
        )
        .await
        .expect("create draft template");
    let author_draft_versions =
        DeployAppApi::list_app_template_versions(&*service, &author_ctx, &draft.id, 1, 20)
            .await
            .expect("author reads their own draft versions");
    assert_eq!(author_draft_versions.items.len(), 1);
    assert_eq!(author_draft_versions.items[0].status, "DRAFT");
    assert_eq!(
        DeployAppApi::retrieve_app_template(&*service, &author_ctx, &draft.id)
            .await
            .expect("author reads their own draft")
            .status,
        "DRAFT"
    );

    // 9. The same draft is indistinguishable from a missing listing to a
    //    consumer: the answer is the same not-found, never an empty page that
    //    would confirm the draft exists.
    let draft_versions_for_consumer =
        DeployAppApi::list_app_template_versions(&*service, &viewer_ctx, &draft.id, 1, 20).await;
    assert!(
        matches!(
            draft_versions_for_consumer,
            Err(DeployServiceError::NotFound(_))
        ),
        "a draft's versions must not be readable by a consumer"
    );
    let draft_for_consumer =
        DeployAppApi::retrieve_app_template(&*service, &viewer_ctx, &draft.id).await;
    assert!(
        matches!(draft_for_consumer, Err(DeployServiceError::NotFound(_))),
        "a draft listing must not be readable by a consumer"
    );

    // 10. A published listing that is not public (`PRIVATE`) is still hidden
    //     from consumers while the author keeps reading it.
    let mut private_request =
        template_request(&format!("private{suffix}"), &app.id, &category.id, "FREE");
    private_request.visibility = Some("PRIVATE".to_owned());
    let private = service
        .create_app_template(&author_ctx, "idem-private", &private_request)
        .await
        .expect("create private template");
    DeployAppApi::submit_app_template(&*service, &author_ctx, &private.id)
        .await
        .expect("submit private template");
    let private_published = DeployBackendApi::update_app_template(
        &*service,
        &operator(),
        &private.id,
        &UpdateAppTemplateAdminRequest {
            status: Some("PUBLISHED".to_owned()),
            review_note: None,
            is_featured: None,
            visibility: None,
        },
    )
    .await
    .expect("approve private template");
    assert_eq!(private_published.visibility, "PRIVATE");
    let private_author_versions =
        DeployAppApi::list_app_template_versions(&*service, &author_ctx, &private.id, 1, 20)
            .await
            .expect("author reads their own private published listing");
    assert_eq!(private_author_versions.items.len(), 1);
    assert_eq!(private_author_versions.items[0].status, "PUBLISHED");
    assert!(
        DeployAppApi::list_app_template_versions(&*service, &viewer_ctx, &private.id, 1, 20)
            .await
            .is_err(),
        "a PRIVATE listing is not a marketplace template"
    );
    assert!(
        DeployAppApi::retrieve_app_template(&*service, &viewer_ctx, &private.id)
            .await
            .is_err(),
        "a PRIVATE listing is not readable by a consumer"
    );
}
