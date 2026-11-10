//! Legacy Cloud Router download-catalog parsing and mapping.
//!
//! The legacy document is read permissively: every field the console could omit
//! is optional here, and one malformed installer never aborts the import. What
//! the import cannot preserve is reported as a warning instead of being invented.

use sdkwork_deploy_contract::{
    UpsertDownloadCardRequest, UpsertDownloadCatalogRequest, UpsertDownloadItemRequest,
    DOWNLOAD_ARCHITECTURES, DOWNLOAD_PLATFORMS,
};
use serde::Deserialize;

/// Operator inputs that are not part of the document.
#[derive(Clone, Debug, Default)]
pub struct ImportOptions {
    /// Catalog key the target site will reference.
    pub catalog_key: String,
    /// Product key override for documents without `product.id`.
    pub product_key: Option<String>,
    /// Channel override for documents without `product.channel`.
    pub channel: Option<String>,
}

/// One mapped installer.
#[derive(Clone, Debug)]
pub struct ItemPlan {
    pub item_key: String,
    pub request: UpsertDownloadItemRequest,
}

/// One mapped card with its installers.
#[derive(Clone, Debug)]
pub struct CardPlan {
    pub card_key: String,
    pub request: UpsertDownloadCardRequest,
    pub items: Vec<ItemPlan>,
}

/// Everything the importer will write for one catalog.
#[derive(Clone, Debug)]
pub struct ImportPlan {
    pub catalog: UpsertDownloadCatalogRequest,
    pub cards: Vec<CardPlan>,
    pub warnings: Vec<String>,
    /// Installers that will be published without an update feed.
    pub installers_without_update: usize,
}

#[derive(Debug, Deserialize)]
struct LegacyCatalog {
    #[serde(default)]
    product: Option<LegacyProduct>,
    #[serde(default)]
    cards: Vec<LegacyCard>,
}

#[derive(Debug, Deserialize)]
struct LegacyProduct {
    #[serde(default)]
    id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    version: String,
    #[serde(default)]
    channel: String,
}

#[derive(Debug, Deserialize)]
struct LegacyCard {
    #[serde(default)]
    id: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    icon: String,
    #[serde(default)]
    tone: String,
    #[serde(default)]
    kind: String,
    #[serde(default)]
    badge: String,
    #[serde(default, rename = "primaryActionStrategy")]
    primary_action_strategy: String,
    #[serde(default)]
    hidden: bool,
    #[serde(default)]
    actions: Vec<LegacyAction>,
}

#[derive(Debug, Deserialize)]
struct LegacyAction {
    #[serde(default)]
    id: String,
    #[serde(default)]
    label: String,
    #[serde(default)]
    platform: String,
    #[serde(default)]
    architecture: String,
    #[serde(default, rename = "fileName")]
    file_name: String,
    #[serde(default)]
    version: String,
    #[serde(default, rename = "releaseTag")]
    release_tag: String,
    #[serde(default)]
    href: String,
    #[serde(default, rename = "sizeBytes")]
    size_bytes: Option<i64>,
    #[serde(default)]
    sha256: String,
    #[serde(default)]
    disabled: bool,
    #[serde(default)]
    primary: bool,
    #[serde(default, rename = "unavailableLabel")]
    unavailable_label: String,
}

fn text(value: &str) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}

fn platform_of(value: &str) -> Option<&'static str> {
    DOWNLOAD_PLATFORMS
        .iter()
        .copied()
        .find(|platform| platform.eq_ignore_ascii_case(value.trim()))
}

fn architecture_of(value: &str) -> Option<&'static str> {
    DOWNLOAD_ARCHITECTURES
        .iter()
        .copied()
        .find(|architecture| architecture.eq_ignore_ascii_case(value.trim()))
}

/// Maps one legacy document to the Deploy write requests.
///
/// @param document - Raw JSON text of the legacy `downloads` document.
/// @param options - Catalog key and the fallbacks for missing product facts.
/// @returns The plan, or an error when the document cannot represent a catalog.
pub fn plan_import(document: &str, options: &ImportOptions) -> Result<ImportPlan, String> {
    if options.catalog_key.trim().is_empty() {
        return Err("catalogKey is required".to_owned());
    }
    let parsed: LegacyCatalog = serde_json::from_str(document)
        .map_err(|error| format!("invalid legacy document: {error}"))?;
    let product = parsed.product.unwrap_or(LegacyProduct {
        id: String::new(),
        name: String::new(),
        version: String::new(),
        channel: String::new(),
    });
    let product_key = options
        .product_key
        .as_deref()
        .and_then(text)
        .or_else(|| text(&product.id))
        .ok_or_else(|| "product.id is missing and no --product-key was provided".to_owned())?;
    let channel = options
        .channel
        .as_deref()
        .and_then(text)
        .or_else(|| text(&product.channel))
        .unwrap_or_else(|| "nightly".to_owned());

    let mut warnings = Vec::new();
    let mut cards = Vec::new();
    let mut installers_without_update = 0;
    for (card_index, card) in parsed.cards.into_iter().enumerate() {
        let card_key = text(&card.id).unwrap_or_else(|| format!("card-{}", card_index + 1));
        let title = text(&card.title).unwrap_or_else(|| card_key.clone());
        let mut items = Vec::new();
        for (action_index, action) in card.actions.into_iter().enumerate() {
            let item_key =
                text(&action.id).unwrap_or_else(|| format!("action-{}", action_index + 1));
            let platform = platform_of(&action.platform);
            let architecture = architecture_of(&action.architecture);
            let (Some(platform), Some(architecture)) = (platform, architecture) else {
                warnings.push(format!(
                    "skipped {card_key}/{item_key}: unsupported target {} / {}",
                    action.platform, action.architecture
                ));
                continue;
            };
            let href = text(&action.href);
            if href.is_none() {
                warnings.push(format!(
                    "{card_key}/{item_key}: no download URL; the installer will import disabled"
                ));
            }
            let label = text(&action.label).unwrap_or_else(|| item_key.clone());
            items.push(ItemPlan {
                item_key: item_key.clone(),
                request: UpsertDownloadItemRequest {
                    platform: Some(platform.to_owned()),
                    architecture: Some(architecture.to_owned()),
                    label: Some(label),
                    file_name: text(&action.file_name),
                    version: text(&action.version),
                    release_tag: text(&action.release_tag),
                    package_id: None,
                    artifact_id: None,
                    download_source: Some(match href {
                        Some(href) => serde_json::json!({ "href": href }),
                        None => serde_json::json!({}),
                    }),
                    size_bytes: action.size_bytes,
                    checksum_sha256: text(&action.sha256),
                    primary_action: Some(action.primary),
                    disabled: Some(action.disabled || href_is_missing(&action.href)),
                    unavailable_label: text(&action.unavailable_label),
                    sort_order: Some(i32::try_from(action_index).unwrap_or(i32::MAX)),
                },
            });
            installers_without_update += 1;
        }
        cards.push(CardPlan {
            card_key,
            request: UpsertDownloadCardRequest {
                title: Some(title),
                description: Some(card.description),
                icon: text(&card.icon),
                tone: text(&card.tone),
                kind: text(&card.kind),
                badge: text(&card.badge),
                primary_strategy: text(&card.primary_action_strategy),
                hidden: Some(card.hidden),
                sort_order: Some(i32::try_from(card_index).unwrap_or(i32::MAX)),
            },
            items,
        });
    }

    Ok(ImportPlan {
        catalog: UpsertDownloadCatalogRequest {
            product_key: Some(product_key),
            product_name: text(&product.name),
            product_version: text(&product.version),
            default_channel_key: Some(channel),
            app_id: None,
        },
        cards,
        warnings,
        installers_without_update,
    })
}

fn href_is_missing(value: &str) -> bool {
    text(value).is_none()
}
