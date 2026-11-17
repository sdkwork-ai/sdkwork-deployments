//! Ownership-mode and endpoint-kind filters, and the response fields that make
//! the three modes distinguishable on the wire.
//!
//! These are pure contract assertions: no database and no HTTP request, so they
//! run in the crate's ordinary `cargo test`. The database side of the same
//! contract (the CHECK constraint and the SQL predicate) is covered by
//! `sdkwork-intelligence-deploy-repository-sqlx/tests/app_ownership_scope_postgres.rs`.

use sdkwork_deploy_contract::{
    AppKind, AppOwnerType, AppResponse, ListAppsAdminQuery, ListAppsQuery,
};

#[test]
fn app_kind_covers_every_endpoint_the_product_registers() {
    let values: Vec<&str> = AppKind::ALL.iter().map(|kind| kind.as_str()).collect();
    // H5, PC, mini program, Flutter, iOS, Android, HarmonyOS, API, desktop.
    for required in [
        "H5_WEB",
        "PC_WEB",
        "WECHAT_MINIPROGRAM",
        "DOUYIN_MINIPROGRAM",
        "FLUTTER_APP",
        "IOS_APP",
        "ANDROID_APP",
        "HARMONYOS_APP",
        "API_SERVICE",
        "DESKTOP_APP",
    ] {
        assert!(values.contains(&required), "{required} must be a kind");
    }

    // Every value round-trips; ALL is the whole vocabulary.
    for kind in AppKind::ALL {
        assert_eq!(AppKind::parse(kind.as_str()), Some(kind));
        // The derive's SCREAMING_SNAKE_CASE spelling must be exactly the value
        // `as_str`/the OpenAPI enum/`chk_deploy_app_app_kind` use — `H5Web` and
        // `PcWeb` are where a naive casing would disagree.
        assert_eq!(
            serde_json::to_string(&kind).expect("kind serializes"),
            format!("\"{}\"", kind.as_str())
        );
    }
    assert_eq!(AppKind::ALL.len(), values.len());

    // The legacy web kinds are still web, and the endpoint-accurate ones are too.
    for kind in [
        AppKind::StaticWeb,
        AppKind::SpaWeb,
        AppKind::H5Web,
        AppKind::PcWeb,
    ] {
        assert!(kind.is_web(), "{} is a web endpoint", kind.as_str());
    }
    assert!(!AppKind::IosApp.is_web());
}

#[test]
fn app_kind_filter_accepts_repeated_and_comma_separated_values() {
    assert_eq!(
        AppKind::parse_filter(["H5_WEB", "PC_WEB"]).unwrap(),
        vec![AppKind::H5Web, AppKind::PcWeb]
    );
    assert_eq!(
        AppKind::parse_filter(["H5_WEB,PC_WEB"]).unwrap(),
        vec![AppKind::H5Web, AppKind::PcWeb]
    );
    // Blank entries are a cleared form field, not a kind.
    assert!(AppKind::parse_filter(["", " ", ","]).unwrap().is_empty());
    // Duplicates collapse.
    assert_eq!(
        AppKind::parse_filter(["IOS_APP", "IOS_APP"]).unwrap(),
        vec![AppKind::IosApp]
    );
    // Unknown values are rejected, never silently matched.
    assert!(AppKind::parse_filter(["NOT_A_KIND"]).is_err());
}

#[test]
fn ownership_scope_merges_its_deprecated_alias() {
    let alias_only = ListAppsQuery {
        scope: Some(AppOwnerType::Organization),
        ..Default::default()
    };
    assert_eq!(
        alias_only.resolved_ownership_scope().unwrap(),
        Some(AppOwnerType::Organization)
    );

    let canonical_only = ListAppsQuery {
        ownership_scope: Some(AppOwnerType::User),
        ..Default::default()
    };
    assert_eq!(
        canonical_only.resolved_ownership_scope().unwrap(),
        Some(AppOwnerType::User)
    );

    let agreeing = ListAppsQuery {
        scope: Some(AppOwnerType::Tenant),
        ownership_scope: Some(AppOwnerType::Tenant),
        ..Default::default()
    };
    assert_eq!(
        agreeing.resolved_ownership_scope().unwrap(),
        Some(AppOwnerType::Tenant)
    );

    let disagreeing = ListAppsQuery {
        scope: Some(AppOwnerType::Tenant),
        ownership_scope: Some(AppOwnerType::User),
        ..Default::default()
    };
    assert!(disagreeing.resolved_ownership_scope().is_err());

    assert_eq!(
        ListAppsQuery::default().resolved_ownership_scope().unwrap(),
        None
    );
}

#[test]
fn organization_filter_reads_zero_as_the_tenant_mode() {
    let tenant = ListAppsQuery {
        owner_organization_id: Some("0".to_owned()),
        ..Default::default()
    };
    assert_eq!(tenant.organization_filter().unwrap(), Some(0));

    let organization = ListAppsQuery {
        owner_organization_id: Some("9001".to_owned()),
        ..Default::default()
    };
    assert_eq!(organization.organization_filter().unwrap(), Some(9001));

    // Absent and blank are both "no filter".
    assert_eq!(
        ListAppsQuery::default().organization_filter().unwrap(),
        None
    );
    let blank = ListAppsQuery {
        owner_organization_id: Some("   ".to_owned()),
        ..Default::default()
    };
    assert_eq!(blank.organization_filter().unwrap(), None);

    for invalid in ["-1", "not-an-id", "+7", "1.5"] {
        let query = ListAppsQuery {
            owner_organization_id: Some(invalid.to_owned()),
            ..Default::default()
        };
        assert!(
            query.organization_filter().is_err(),
            "{invalid} must be rejected"
        );
    }
}

#[test]
fn admin_query_maps_onto_the_shared_facet_set() {
    let admin = ListAppsAdminQuery {
        page: 2,
        page_size: 50,
        keyword: Some("deploy".to_owned()),
        ownership_scope: Some(AppOwnerType::Organization),
        owner_organization_id: Some("9001".to_owned()),
        tenant_id: Some("7".to_owned()),
        app_kind: vec![AppKind::FlutterApp],
        ..Default::default()
    };
    let filter = admin.filter();
    assert_eq!(filter.page, 2);
    assert_eq!(filter.page_size, 50);
    assert_eq!(filter.keyword.as_deref(), Some("deploy"));
    assert_eq!(filter.ownership_scope, Some(AppOwnerType::Organization));
    assert_eq!(filter.owner_organization_id.as_deref(), Some("9001"));
    assert_eq!(filter.app_kind, vec![AppKind::FlutterApp]);

    assert_eq!(admin.tenant_filter().unwrap(), Some(7));
    assert_eq!(ListAppsAdminQuery::default().tenant_filter().unwrap(), None);
    for invalid in ["0", "-7", "tenant-a"] {
        let query = ListAppsAdminQuery {
            tenant_id: Some(invalid.to_owned()),
            ..Default::default()
        };
        assert!(
            query.tenant_filter().is_err(),
            "{invalid} must be rejected as a tenant id"
        );
    }
}

#[test]
fn the_response_reports_both_names_and_the_organization_pointer() {
    let tenant = AppResponse {
        owner_type: AppOwnerType::Tenant,
        ownership_scope: AppOwnerType::Tenant,
        organization_id: "0".to_owned(),
        ..Default::default()
    };
    let json = serde_json::to_value(&tenant).expect("AppResponse serializes");
    assert_eq!(json["ownerType"], "TENANT");
    assert_eq!(json["ownershipScope"], "TENANT");
    // `"0"` is returned, not omitted: it is the tenant-mode encoding.
    assert_eq!(json["organizationId"], "0");
    assert!(json.get("ownerUserId").is_none());

    let organization = AppResponse {
        owner_type: AppOwnerType::Organization,
        ownership_scope: AppOwnerType::Organization,
        organization_id: "9001".to_owned(),
        ..Default::default()
    };
    let json = serde_json::to_value(&organization).expect("AppResponse serializes");
    assert_eq!(json["ownershipScope"], "ORGANIZATION");
    assert_eq!(json["organizationId"], "9001");

    let personal = AppResponse {
        owner_type: AppOwnerType::User,
        ownership_scope: AppOwnerType::User,
        organization_id: "0".to_owned(),
        owner_user_id: Some("11001".to_owned()),
        ..Default::default()
    };
    let json = serde_json::to_value(&personal).expect("AppResponse serializes");
    assert_eq!(json["ownershipScope"], "USER");
    assert_eq!(json["organizationId"], "0");
    assert_eq!(json["ownerUserId"], "11001");
}
