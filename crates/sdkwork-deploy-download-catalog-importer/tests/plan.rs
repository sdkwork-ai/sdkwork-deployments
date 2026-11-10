use sdkwork_deploy_download_catalog_importer::{plan_import, ImportOptions};

fn legacy(document: serde_json::Value) -> String {
    serde_json::to_string(&document).expect("serialize fixture")
}

fn options() -> ImportOptions {
    ImportOptions {
        catalog_key: "cloudrouter-website".to_owned(),
        product_key: None,
        channel: None,
    }
}

#[test]
fn maps_product_cards_and_installers() {
    let document = legacy(serde_json::json!({
        "product": { "id": "app.cloudrouter", "name": "Cloud Router", "version": "0.4.12", "channel": "stable" },
        "cards": [{
            "id": "desktop",
            "title": "Desktop",
            "description": "Installers",
            "actions": [
                { "id": "win-x64", "label": "Windows x64", "platform": "windows", "architecture": "x64",
                  "fileName": "cloudrouter.exe", "version": "0.4.12", "href": "https://download.example.com/a.exe",
                  "sizeBytes": 100, "sha256": "abc", "primary": true },
                { "id": "mac-arm64", "label": "macOS arm64", "platform": "macos", "architecture": "arm64",
                  "href": "https://download.example.com/a.dmg" }
            ]
        }]
    }));
    let plan = plan_import(&document, &options()).expect("plan");
    assert_eq!(plan.catalog.product_key.as_deref(), Some("app.cloudrouter"));
    assert_eq!(plan.catalog.default_channel_key.as_deref(), Some("stable"));
    assert_eq!(plan.cards.len(), 1);
    assert_eq!(plan.cards[0].card_key, "desktop");
    assert_eq!(plan.cards[0].items.len(), 2);
    let windows = &plan.cards[0].items[0];
    assert_eq!(windows.request.platform.as_deref(), Some("windows"));
    assert_eq!(windows.request.primary_action, Some(true));
    assert_eq!(windows.request.disabled, Some(false));
    assert_eq!(
        windows
            .request
            .download_source
            .as_ref()
            .and_then(|value| value.get("href")),
        Some(&serde_json::json!("https://download.example.com/a.exe"))
    );
    assert_eq!(plan.installers_without_update, 2);
    assert!(plan.warnings.is_empty());
}

#[test]
fn skips_unsupported_targets_with_a_warning() {
    let document = legacy(serde_json::json!({
        "product": { "id": "app" },
        "cards": [{ "id": "desktop", "actions": [
            { "id": "plan9", "platform": "plan9", "architecture": "x64", "href": "https://example.com/a" }
        ] }]
    }));
    let plan = plan_import(&document, &options()).expect("plan");
    assert!(plan.cards[0].items.is_empty());
    assert!(plan
        .warnings
        .iter()
        .any(|warning| warning.contains("plan9")));
}

#[test]
fn marks_installer_without_url_disabled() {
    let document = legacy(serde_json::json!({
        "product": { "id": "app" },
        "cards": [{ "id": "desktop", "actions": [
            { "id": "win-x64", "platform": "windows", "architecture": "x64" }
        ] }]
    }));
    let plan = plan_import(&document, &options()).expect("plan");
    let item = &plan.cards[0].items[0];
    assert_eq!(item.request.disabled, Some(true));
    assert!(plan
        .warnings
        .iter()
        .any(|warning| warning.contains("no download URL")));
}

#[test]
fn requires_a_product_key() {
    let document = legacy(serde_json::json!({ "cards": [] }));
    let error = plan_import(&document, &options()).expect_err("must fail");
    assert!(error.contains("product.id"));
    let mut overridden = options();
    overridden.product_key = Some("app.sdkwork.birdcoder".to_owned());
    let plan = plan_import(&document, &overridden).expect("plan");
    assert_eq!(
        plan.catalog.product_key.as_deref(),
        Some("app.sdkwork.birdcoder")
    );
    assert_eq!(plan.catalog.default_channel_key.as_deref(), Some("nightly"));
}

#[test]
fn requires_a_catalog_key() {
    let document = legacy(serde_json::json!({ "product": { "id": "app" }, "cards": [] }));
    let error = plan_import(&document, &ImportOptions::default()).expect_err("must fail");
    assert!(error.contains("catalogKey"));
}

#[test]
fn rejects_malformed_json() {
    let error = plan_import("{ not json", &options()).expect_err("must fail");
    assert!(error.contains("invalid legacy document"));
}
