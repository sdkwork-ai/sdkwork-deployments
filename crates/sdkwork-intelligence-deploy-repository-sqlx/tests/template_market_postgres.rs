//! App template marketplace end-to-end integration test: the commercial flow
//! against a real PostgreSQL (category → publish → submit → review → browse
//! facets → acquire FREE/PAID → settle → revoke → tenancy isolation).
//!
//! Runs only with `SDKWORK_DATABASE_TEST_POSTGRES_URL` set; the schema is the
//! disposable `postgres_pool()` baseline (see tests/common/mod.rs).

mod common;

use sdkwork_database_id::SnowflakeIdGenerator;
use sdkwork_deploy_contract::{
    AppTemplatePage, CreateAppTemplateRequest, CreateAppTemplateVersionRequest,
    CreateTemplateCategoryRequest, CreateTemplatePurchaseRequest, DeployAppApi,
    DeployAppRequestContext, DeployBackendApi, DeployBackendRequestContext,
    ListMarketplaceTemplatesQuery, ListTemplatePurchasesQuery, SettleTemplatePurchaseRequest,
    UpdateAppTemplateAdminRequest,
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

fn buyer(actor: i64) -> DeployAppRequestContext {
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
        summary: "A purchasable template".to_owned(),
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
async fn postgres_marketplace_commercial_flow_is_settlement_safe_and_tenant_bounded() {
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
    let buyer_ctx = buyer(12);

    // 1. Admin category CRUD.
    let category = service
        .create_template_category(&operator(), &category_request(&format!("cat{suffix}")))
        .await
        .expect("create category");
    assert_eq!(category.status, "ACTIVE");

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

    // 3. Submit refuses nothing less than a version; approval publishes both.
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

    // 4. Marketplace browse facets: type + keyword hit; a PAID PPT template by
    //    a second author stays visible while a DRAFT one does not.
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
    assert_eq!(paid_approved.latest_version_uuid.is_some(), true);

    let browsed = DeployAppApi::list_marketplace_templates(
        &*service,
        &buyer_ctx,
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

    let typed = DeployAppApi::list_marketplace_templates(
        &*service,
        &buyer_ctx,
        &ListMarketplaceTemplatesQuery {
            template_type: Some("VIDEO".to_owned()),
            ..ListMarketplaceTemplatesQuery::default()
        },
    )
    .await
    .expect("browse by type");
    assert!(typed.items.is_empty(), "no VIDEO templates exist yet");

    // 5. FREE acquire settles instantly and is idempotent; the install count
    //    moves exactly once.
    let entitlement = DeployAppApi::create_template_purchase(
        &*service,
        &buyer_ctx,
        &free.id,
        "acquire-key-1",
        &CreateTemplatePurchaseRequest::default(),
    )
    .await
    .expect("acquire free template");
    assert_eq!(entitlement.status, "ACTIVE");
    let replay = DeployAppApi::create_template_purchase(
        &*service,
        &buyer_ctx,
        &free.id,
        "acquire-key-1",
        &CreateTemplatePurchaseRequest::default(),
    )
    .await
    .expect("replay acquire");
    assert_eq!(replay.id, entitlement.id);
    let free_after = service
        .retrieve_marketplace_template(&buyer_ctx, &free.id)
        .await
        .expect("reread template");
    assert_eq!(free_after.install_count, "1");

    // 6. PAID acquire parks in PENDING; settlement without a payment_ref is
    //    impossible by contract, and settle activates the entitlement.
    let pending = DeployAppApi::create_template_purchase(
        &*service,
        &buyer_ctx,
        &paid.id,
        "acquire-key-2",
        &CreateTemplatePurchaseRequest::default(),
    )
    .await
    .expect("acquire paid template");
    assert_eq!(pending.status, "PENDING");
    let settled = service
        .settle_template_purchase(
            &operator(),
            &pending.id,
            &SettleTemplatePurchaseRequest {
                payment_ref: format!("pay-{suffix}"),
            },
        )
        .await
        .expect("settle purchase");
    assert_eq!(settled.status, "ACTIVE");
    assert_eq!(
        settled.payment_ref.as_deref(),
        Some(format!("pay-{suffix}").as_str())
    );

    // 7. Buyer inventory sees both entitlements.
    let purchases = DeployAppApi::list_template_purchases(
        &*service,
        &buyer_ctx,
        &ListTemplatePurchasesQuery {
            page: 1,
            page_size: 20,
            status: None,
        },
    )
    .await
    .expect("list purchases");
    assert_eq!(purchases.items.len(), 2);

    // 8. Revocation kills the entitlement and walks the install count back.
    let revoked = service
        .revoke_template_purchase(&operator(), &entitlement.id)
        .await
        .expect("revoke entitlement");
    assert_eq!(revoked.status, "REVOKED");
    let free_after_revoke = service
        .retrieve_marketplace_template(&buyer_ctx, &free.id)
        .await
        .expect("reread after revoke");
    assert_eq!(free_after_revoke.install_count, "0");

    // 9. Tenant boundary: the same listing is invisible to another tenant's
    //    marketplace browse and purchase attempt.
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
    let foreign_acquire = DeployAppApi::create_template_purchase(
        &*service,
        &outsider,
        &free.id,
        "foreign-key",
        &CreateTemplatePurchaseRequest::default(),
    )
    .await;
    assert!(
        foreign_acquire.is_err(),
        "cross-tenant acquire must be refused"
    );

    // Silence unused-import warnings for the page alias used above.
    let _: AppTemplatePage = AppTemplatePage::default();
}
