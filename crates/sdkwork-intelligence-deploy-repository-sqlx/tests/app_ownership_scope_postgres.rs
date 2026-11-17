//! `deploy_app` ownership modes and endpoint-kind filters.
//!
//! The three registration modes are mutually exclusive in the database
//! (`chk_deploy_app_ownership_alignment`) and filterable on both ledgers. These
//! tests pin the parts that cannot be checked from the contract alone:
//!
//! 1. the DDL rejects every shape that mixes two modes,
//! 2. an application created without an explicit mode is a **tenant**
//!    application (`organization_id = 0`), not the creating user's personal one,
//! 3. `ownership_scope` / `organization_id` / `app_kind` narrow without widening
//!    reach, and
//! 4. another tenant's rows, and another person's personal apps, stay invisible.
//!
//! Requires `SDKWORK_DATABASE_TEST_POSTGRES_URL`; ignored by default like the
//! other PostgreSQL integration tests in this crate.

mod common;

use sdkwork_deploy_contract::{
    AppKind, AppOwnerType, AppResponse, CreateAppRequest, DeployServiceErrorKind, ListAppsQuery,
};
use sdkwork_intelligence_deploy_service::DeployRepositoryPort;
use sqlx::PgPool;

const TENANT_A: i64 = 7001;
const TENANT_B: i64 = 7002;
const ORG_A: i64 = 9001;
const ALICE: i64 = 11001;
const BOB: i64 = 11002;

fn app_request(
    name: &str,
    slug: &str,
    app_kind: AppKind,
    owner_type: Option<AppOwnerType>,
) -> CreateAppRequest {
    let mut payload = serde_json::json!({
        "name": name,
        "slug": slug,
        "appKind": app_kind.as_str(),
    });
    if let Some(owner_type) = owner_type {
        payload["ownerType"] = serde_json::json!(owner_type.as_str());
    }
    serde_json::from_value(payload).expect("CreateAppRequest parses from its own wire shape")
}

fn quiet_filters() -> ListAppsQuery {
    ListAppsQuery {
        page: 1,
        page_size: 100,
        ..Default::default()
    }
}

fn uuid_for(suffix: i64) -> String {
    format!("00000000-0000-4000-8000-{suffix:012}")
}

async fn insert_raw_app(
    pool: &PgPool,
    id: i64,
    uuid: &str,
    tenant_id: i64,
    organization_id: i64,
    owner_type: &str,
    user_id: Option<i64>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO deploy_app (id, uuid, tenant_id, organization_id, name, slug, app_kind,
                                 app_status, type, runtime_config, metadata, data_scope,
                                 default_environment, owner_type, user_id,
                                 created_at, updated_at, version)
         VALUES ($1, $2, $3, $4, $5, $5, 'STATIC_WEB', 'ACTIVE', 1, '{}', '{}', 1,
                 'production', $6, $7, NOW(), NOW(), 1)",
    )
    .bind(id)
    .bind(uuid)
    .bind(tenant_id)
    .bind(organization_id)
    .bind(format!("invalid-shape-{id}"))
    .bind(owner_type)
    .bind(user_id)
    .execute(pool)
    .await
    .map(|_| ())
}

fn failure_kind<T: std::fmt::Debug>(
    result: Result<T, sdkwork_deploy_contract::DeployServiceError>,
) -> DeployServiceErrorKind {
    match result {
        Ok(value) => panic!("expected the operation to fail, got {value:?}"),
        Err(error) => error.kind(),
    }
}

#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn an_application_created_without_a_mode_is_a_tenant_application() {
    let (repository, pool) =
        common::migrated_repository_with_pool("sdkwork-deploy-ownership-default-test").await;

    let app = repository
        .create_app(
            TENANT_A,
            Some(ORG_A),
            Some(ALICE),
            None,
            &app_request("Tenant App", "tenant-app", AppKind::H5Web, None),
        )
        .await
        .expect("create without an ownership mode");

    // The product default: "not registered against an organization" means the
    // current tenant's application, encoded as organization_id = 0.
    assert_eq!(app.ownership_scope, AppOwnerType::Tenant);
    assert_eq!(app.owner_type, AppOwnerType::Tenant);
    assert_eq!(app.organization_id, "0");
    assert!(app.owner_user_id.is_none());

    // The pointer is the stored column, not a read-time normalisation.
    let stored: (String, i64, Option<i64>) = sqlx::query_as(
        "SELECT owner_type, organization_id, user_id FROM deploy_app WHERE uuid = $1",
    )
    .bind(&app.id)
    .fetch_one(&pool)
    .await
    .expect("read the created row");
    assert_eq!(stored, ("TENANT".to_owned(), 0, None));
}

#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn explicit_modes_store_the_pointers_their_mode_requires() {
    let (repository, _pool) =
        common::migrated_repository_with_pool("sdkwork-deploy-ownership-explicit-test").await;

    let personal = repository
        .create_app(
            TENANT_A,
            Some(ORG_A),
            Some(ALICE),
            None,
            &app_request(
                "Personal App",
                "personal-app",
                AppKind::AndroidApp,
                Some(AppOwnerType::User),
            ),
        )
        .await
        .expect("create a personal application");
    assert_eq!(personal.ownership_scope, AppOwnerType::User);
    assert_eq!(personal.organization_id, "0");
    assert_eq!(personal.owner_user_id.as_deref(), Some("11001"));

    let organized = repository
        .create_app(
            TENANT_A,
            Some(ORG_A),
            Some(ALICE),
            None,
            &app_request(
                "Org App",
                "org-app",
                AppKind::IosApp,
                Some(AppOwnerType::Organization),
            ),
        )
        .await
        .expect("create an organization application");
    assert_eq!(organized.ownership_scope, AppOwnerType::Organization);
    assert_eq!(organized.organization_id, "9001");
    assert!(organized.owner_user_id.is_none());

    // An organization app needs an organization, and a personal app needs a
    // subject; neither degrades silently to another mode.
    let no_org = repository
        .create_app(
            TENANT_A,
            None,
            Some(ALICE),
            None,
            &app_request(
                "No Org",
                "no-org",
                AppKind::PcWeb,
                Some(AppOwnerType::Organization),
            ),
        )
        .await;
    assert_eq!(failure_kind(no_org), DeployServiceErrorKind::Validation);

    let no_subject = repository
        .create_app(
            TENANT_A,
            Some(ORG_A),
            None,
            None,
            &app_request(
                "No Subject",
                "no-subject",
                AppKind::PcWeb,
                Some(AppOwnerType::User),
            ),
        )
        .await;
    assert_eq!(failure_kind(no_subject), DeployServiceErrorKind::Validation);
}

#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn the_database_rejects_rows_that_mix_two_ownership_modes() {
    let (_repository, pool) =
        common::migrated_repository_with_pool("sdkwork-deploy-ownership-check-test").await;

    // TENANT with a real organization.
    let tenant_with_org = insert_raw_app(
        &pool,
        81_001,
        &uuid_for(81_001),
        TENANT_A,
        ORG_A,
        "TENANT",
        None,
    )
    .await;
    assert!(
        tenant_with_org.is_err(),
        "TENANT must carry organization_id = 0"
    );

    // ORGANIZATION with no organization.
    let org_without_org = insert_raw_app(
        &pool,
        81_002,
        &uuid_for(81_002),
        TENANT_A,
        0,
        "ORGANIZATION",
        None,
    )
    .await;
    assert!(
        org_without_org.is_err(),
        "ORGANIZATION must carry a positive organization_id"
    );

    // USER with no user.
    let user_without_user =
        insert_raw_app(&pool, 81_003, &uuid_for(81_003), TENANT_A, 0, "USER", None).await;
    assert!(user_without_user.is_err(), "USER must carry user_id");

    // USER that still points at an organization.
    let user_with_org = insert_raw_app(
        &pool,
        81_004,
        &uuid_for(81_004),
        TENANT_A,
        ORG_A,
        "USER",
        Some(ALICE),
    )
    .await;
    assert!(
        user_with_org.is_err(),
        "USER must not carry an organization"
    );

    // The valid shapes still insert: the constraint rejects only mixed modes.
    for (id, owner_type, organization_id, user_id) in [
        (81_011, "TENANT", 0, None),
        (81_012, "ORGANIZATION", ORG_A, None),
        (81_013, "USER", 0, Some(ALICE)),
    ] {
        insert_raw_app(
            &pool,
            id,
            &uuid_for(id),
            TENANT_A,
            organization_id,
            owner_type,
            user_id,
        )
        .await
        .unwrap_or_else(|error| panic!("{owner_type} row must be accepted: {error}"));
    }
}

async fn seed_ledger(
    repository: &sdkwork_intelligence_deploy_repository_sqlx::DeployRepository,
) -> (AppResponse, AppResponse, AppResponse) {
    let tenant_app = repository
        .create_app(
            TENANT_A,
            Some(ORG_A),
            Some(ALICE),
            None,
            &app_request("H5 Ledger", "h5-ledger", AppKind::H5Web, None),
        )
        .await
        .expect("tenant app");
    let org_app = repository
        .create_app(
            TENANT_A,
            Some(ORG_A),
            Some(ALICE),
            None,
            &app_request(
                "IOS Ledger",
                "ios-ledger",
                AppKind::IosApp,
                Some(AppOwnerType::Organization),
            ),
        )
        .await
        .expect("organization app");
    let user_app = repository
        .create_app(
            TENANT_A,
            Some(ORG_A),
            Some(ALICE),
            None,
            &app_request(
                "Android Ledger",
                "android-ledger",
                AppKind::AndroidApp,
                Some(AppOwnerType::User),
            ),
        )
        .await
        .expect("personal app");
    (tenant_app, org_app, user_app)
}

fn listed_ids(page: &sdkwork_deploy_contract::AppPage) -> Vec<String> {
    page.items.iter().map(|app| app.id.clone()).collect()
}

#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn ownership_and_endpoint_filters_narrow_the_tenant_ledger() {
    let (repository, _pool) =
        common::migrated_repository_with_pool("sdkwork-deploy-ownership-filter-test").await;
    let (tenant_app, org_app, user_app) = seed_ledger(&repository).await;

    // No facet: every mode the caller can reach.
    let all = repository
        .list_apps(TENANT_A, Some(ALICE), Some(ORG_A), &quiet_filters())
        .await
        .expect("list every reachable app");
    assert_eq!(all.items.len(), 3);

    // ownership_scope = TENANT.
    let tenant_only = repository
        .list_apps(
            TENANT_A,
            Some(ALICE),
            Some(ORG_A),
            &ListAppsQuery {
                ownership_scope: Some(AppOwnerType::Tenant),
                ..quiet_filters()
            },
        )
        .await
        .expect("filter by tenant mode");
    assert_eq!(listed_ids(&tenant_only), vec![tenant_app.id.clone()]);

    // organization_id = 0 means the tenant mode as well.
    let tenant_by_org = repository
        .list_apps(
            TENANT_A,
            Some(ALICE),
            Some(ORG_A),
            &ListAppsQuery {
                owner_organization_id: Some("0".to_owned()),
                ..quiet_filters()
            },
        )
        .await
        .expect("filter by organization_id 0");
    assert_eq!(listed_ids(&tenant_by_org), vec![tenant_app.id.clone()]);

    // organization_id = <org> is the organization mode in that organization.
    let org_only = repository
        .list_apps(
            TENANT_A,
            Some(ALICE),
            Some(ORG_A),
            &ListAppsQuery {
                owner_organization_id: Some(ORG_A.to_string()),
                ..quiet_filters()
            },
        )
        .await
        .expect("filter by organization id");
    assert_eq!(listed_ids(&org_only), vec![org_app.id.clone()]);

    // app_kind is multi-valued and OR-ed inside itself.
    let mobile_endpoints = repository
        .list_apps(
            TENANT_A,
            Some(ALICE),
            Some(ORG_A),
            &ListAppsQuery {
                app_kind: vec![AppKind::IosApp, AppKind::AndroidApp],
                ..quiet_filters()
            },
        )
        .await
        .expect("filter by two endpoint kinds");
    let mut expected = vec![org_app.id.clone(), user_app.id.clone()];
    expected.sort();
    let mut actual = listed_ids(&mobile_endpoints);
    actual.sort();
    assert_eq!(actual, expected);

    // Facets are conjunctive, and an empty intersection is empty rather than
    // silently ignored.
    let empty = repository
        .list_apps(
            TENANT_A,
            Some(ALICE),
            Some(ORG_A),
            &ListAppsQuery {
                ownership_scope: Some(AppOwnerType::Organization),
                app_kind: vec![AppKind::H5Web],
                ..quiet_filters()
            },
        )
        .await
        .expect("filter by mode and kind");
    assert!(empty.items.is_empty());

    // The legacy `scope` alias and the new name must agree.
    let conflict = repository
        .list_apps(
            TENANT_A,
            Some(ALICE),
            Some(ORG_A),
            &ListAppsQuery {
                scope: Some(AppOwnerType::Tenant),
                ownership_scope: Some(AppOwnerType::User),
                ..quiet_filters()
            },
        )
        .await;
    assert_eq!(failure_kind(conflict), DeployServiceErrorKind::Validation);

    // A malformed organization id is rejected, not treated as "no rows".
    let malformed = repository
        .list_apps(
            TENANT_A,
            Some(ALICE),
            Some(ORG_A),
            &ListAppsQuery {
                owner_organization_id: Some("not-an-id".to_owned()),
                ..quiet_filters()
            },
        )
        .await;
    assert_eq!(failure_kind(malformed), DeployServiceErrorKind::Validation);
}

#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn every_endpoint_kind_is_accepted_by_the_database_and_reads_back_verbatim() {
    let (repository, _pool) =
        common::migrated_repository_with_pool("sdkwork-deploy-app-kind-vocabulary-test").await;

    for (index, kind) in AppKind::ALL.into_iter().enumerate() {
        let name = format!("Endpoint {index}");
        let slug = format!("endpoint-{index}");
        let app = repository
            .create_app(
                TENANT_A,
                None,
                Some(ALICE),
                None,
                &app_request(&name, &slug, kind, None),
            )
            .await
            .unwrap_or_else(|error| {
                panic!("{kind:?} must be a registrable endpoint kind: {error}")
            });
        assert_eq!(
            app.app_kind,
            kind.as_str(),
            "the stored value must be the contract's spelling"
        );
    }
}

#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn another_tenants_and_another_persons_apps_stay_invisible() {
    let (repository, _pool) =
        common::migrated_repository_with_pool("sdkwork-deploy-ownership-reach-test").await;

    let (_, _, alice_personal) = seed_ledger(&repository).await;
    let bob_personal = repository
        .create_app(
            TENANT_A,
            Some(ORG_A),
            Some(BOB),
            None,
            &app_request(
                "Bob Personal",
                "bob-personal",
                AppKind::WechatMiniprogram,
                Some(AppOwnerType::User),
            ),
        )
        .await
        .expect("bob's personal app");
    let other_tenant = repository
        .create_app(
            TENANT_B,
            None,
            Some(BOB),
            None,
            &app_request("Other Tenant", "other-tenant", AppKind::PcWeb, None),
        )
        .await
        .expect("another tenant's app");

    let alice_view = repository
        .list_apps(TENANT_A, Some(ALICE), Some(ORG_A), &quiet_filters())
        .await
        .expect("alice lists her tenant");
    assert!(alice_view
        .items
        .iter()
        .any(|app| app.id == alice_personal.id));
    assert!(
        !alice_view.items.iter().any(|app| app.id == bob_personal.id),
        "another person's personal app must not be listed"
    );
    assert!(
        !alice_view.items.iter().any(|app| app.id == other_tenant.id),
        "another tenant's app must not be listed"
    );

    let bob_view = repository
        .list_apps(TENANT_A, Some(BOB), Some(ORG_A), &quiet_filters())
        .await
        .expect("bob lists his tenant");
    assert!(bob_view.items.iter().any(|app| app.id == bob_personal.id));

    // The operator ledger has no owner gate and, with no tenant in its
    // context, spans every tenant.
    let operator_view = repository
        .list_apps_admin(None, &quiet_filters())
        .await
        .expect("operator lists every tenant");
    assert!(operator_view
        .items
        .iter()
        .any(|app| app.id == other_tenant.id));

    let tenant_scoped = repository
        .list_apps_admin(Some(TENANT_B), &quiet_filters())
        .await
        .expect("operator lists one tenant");
    assert_eq!(listed_ids(&tenant_scoped), vec![other_tenant.id.clone()]);
}
