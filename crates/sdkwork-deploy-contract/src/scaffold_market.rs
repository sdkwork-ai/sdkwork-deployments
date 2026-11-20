//! App scaffold marketplace catalog wire contracts.
//!
//! An author publishes a `deploy_app` as a scaffold listing (`ApplicationScaffold`),
//! optionally with versions (`ApplicationScaffoldVersion`); the marketplace browses the
//! tenant's `PUBLIC` + `PUBLISHED` listings. This module owns the catalog only:
//! categories, listings and versions. Acquisition, payment and entitlement are
//! owned by the sdkwork-order order center and sdkwork-payment, so no purchase,
//! order or entitlement contract lives here. Everything is tenant-scoped: the
//! marketplace browse surface only ever sees the caller's tenant's listings.

use serde::{Deserialize, Serialize};

/// Install a scaffold as a new application.
///
/// This is the **free, Git-backed** install path. Archive-backed and paid
/// scaffolds are refused until artifact sharing and order/payment entitlement
/// are wired, so the API never creates an app whose source it cannot deliver.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InstallApplicationScaffoldRequest {
    /// Name of the created application; defaults to the scaffold's display name.
    #[serde(default)]
    pub name: Option<String>,
    /// Optional slug of the created application.
    #[serde(default)]
    pub slug: Option<String>,
    /// Paid order that owns this listing. Required for `PAID` scaffolds; the
    /// repository verifies it against `commerce_order` before creating an app.
    #[serde(default, rename = "orderId")]
    pub order_id: Option<String>,
}

/// The application created from a scaffold.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplicationScaffoldInstallResponse {
    pub app: crate::AppResponse,
    #[serde(rename = "scaffoldUuid")]
    pub scaffold_uuid: String,
}
/// Wire vocabulary for `deploy_application_scaffold.visibility`.
pub const SCAFFOLD_VISIBILITY_PUBLIC: &str = "PUBLIC";
pub const SCAFFOLD_VISIBILITY_PRIVATE: &str = "PRIVATE";

/// Wire vocabulary for `deploy_application_scaffold.pricing_model`.
pub const SCAFFOLD_PRICING_FREE: &str = "FREE";
pub const SCAFFOLD_PRICING_PAID: &str = "PAID";

/// Wire vocabulary for `deploy_application_scaffold.status`.
pub const SCAFFOLD_STATUS_DRAFT: &str = "DRAFT";
pub const SCAFFOLD_STATUS_PENDING_REVIEW: &str = "PENDING_REVIEW";
pub const SCAFFOLD_STATUS_PUBLISHED: &str = "PUBLISHED";
pub const SCAFFOLD_STATUS_REJECTED: &str = "REJECTED";
pub const SCAFFOLD_STATUS_DISABLED: &str = "DISABLED";

/// Wire vocabulary for `deploy_application_scaffold_version.status`.
pub const SCAFFOLD_VERSION_STATUS_DRAFT: &str = "DRAFT";
pub const SCAFFOLD_VERSION_STATUS_PUBLISHED: &str = "PUBLISHED";
pub const SCAFFOLD_VERSION_STATUS_WITHDRAWN: &str = "WITHDRAWN";

/// Wire vocabulary for `deploy_application_scaffold_category.status`.
pub const SCAFFOLD_CATEGORY_STATUS_ACTIVE: &str = "ACTIVE";
pub const SCAFFOLD_CATEGORY_STATUS_DISABLED: &str = "DISABLED";

/// Wire vocabulary for `deploy_application_scaffold.template_type`: the artifact
/// form a scaffold was conversationally created as (project-file authoring).
/// Extending the vocabulary is a baseline CHECK + contract enum change.
pub const SCAFFOLD_TYPE_APP: &str = "APP";
pub const SCAFFOLD_TYPE_PPT: &str = "PPT";
pub const SCAFFOLD_TYPE_VIDEO: &str = "VIDEO";

/// Marketplace browse ordering. `POPULAR` ranks by install count, `NEWEST`
/// (the default) by recency.
pub const MARKETPLACE_SORT_NEWEST: &str = "NEWEST";
pub const MARKETPLACE_SORT_POPULAR: &str = "POPULAR";

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ScaffoldCategoryResponse {
    pub id: String,
    #[serde(rename = "categoryKey")]
    pub category_key: String,
    #[serde(rename = "parentId", skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    #[serde(rename = "displayName")]
    pub display_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(rename = "sortOrder")]
    pub sort_order: i32,
    pub status: String,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    #[serde(rename = "updatedAt")]
    pub updated_at: String,
    pub version: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ScaffoldCategoryPage {
    pub items: Vec<ScaffoldCategoryResponse>,
    pub total: i64,
    pub page: i32,
    pub page_size: i32,
}

/// One marketplace sample page. The media reference points at a Drive-backed
/// image; `sortOrder` preserves the author's page order independently of the
/// JSON array order.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ApplicationScaffoldScreenshot {
    #[serde(rename = "mediaRef")]
    pub media_ref: String,
    /// Store shelf the page documents (for example `iphone-69` or
    /// `android-phone`). Optional for older rows and non-device listings.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caption: Option<String>,
    #[serde(rename = "sortOrder", default)]
    pub sort_order: i32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ApplicationScaffoldSummaryResponse {
    pub id: String,
    #[serde(rename = "templateType")]
    pub template_type: String,
    #[serde(rename = "templateKey")]
    pub template_key: String,
    #[serde(rename = "displayName")]
    pub display_name: String,
    pub summary: String,
    #[serde(rename = "categoryUuid")]
    pub category_uuid: String,
    #[serde(rename = "iconMediaRef", skip_serializing_if = "Option::is_none")]
    pub icon_media_ref: Option<String>,
    #[serde(rename = "coverMediaRef", skip_serializing_if = "Option::is_none")]
    pub cover_media_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub screenshots: Vec<ApplicationScaffoldScreenshot>,
    pub visibility: String,
    #[serde(rename = "pricingModel")]
    pub pricing_model: String,
    /// Minor-unit price; API_SPEC §13.6 puts the int64 amount on the wire as a
    /// string and §13.2 tags it `x-sdkwork-money-unit: minor`.
    #[serde(rename = "priceMinor")]
    pub price_minor: String,
    pub currency: String,
    pub status: String,
    #[serde(rename = "isFeatured")]
    pub is_featured: bool,
    #[serde(rename = "installCount")]
    pub install_count: String,
    #[serde(rename = "viewCount")]
    pub view_count: String,
    #[serde(rename = "latestVersionUuid", skip_serializing_if = "Option::is_none")]
    pub latest_version_uuid: Option<String>,
    #[serde(rename = "updatedAt")]
    pub updated_at: String,
    pub version: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ApplicationScaffoldSummaryPage {
    pub items: Vec<ApplicationScaffoldSummaryResponse>,
    pub total: i64,
    pub page: i32,
    pub page_size: i32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ApplicationScaffoldResponse {
    pub id: String,
    #[serde(rename = "templateType")]
    pub template_type: String,
    #[serde(rename = "templateKey")]
    pub template_key: String,
    #[serde(rename = "displayName")]
    pub display_name: String,
    pub summary: String,
    pub description: String,
    #[serde(rename = "appUuid")]
    pub app_uuid: String,
    #[serde(rename = "categoryUuid")]
    pub category_uuid: String,
    /// Author user subject; int64-as-string per API_SPEC §13.6.
    #[serde(rename = "authorUserId")]
    pub author_user_id: String,
    #[serde(rename = "iconMediaRef", skip_serializing_if = "Option::is_none")]
    pub icon_media_ref: Option<String>,
    #[serde(rename = "coverMediaRef", skip_serializing_if = "Option::is_none")]
    pub cover_media_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub screenshots: Vec<ApplicationScaffoldScreenshot>,
    pub visibility: String,
    #[serde(rename = "pricingModel")]
    pub pricing_model: String,
    #[serde(rename = "priceMinor")]
    pub price_minor: String,
    pub currency: String,
    pub status: String,
    #[serde(rename = "reviewNote", skip_serializing_if = "Option::is_none")]
    pub review_note: Option<String>,
    #[serde(rename = "isFeatured")]
    pub is_featured: bool,
    #[serde(rename = "installCount")]
    pub install_count: String,
    #[serde(rename = "viewCount")]
    pub view_count: String,
    #[serde(rename = "latestVersionUuid", skip_serializing_if = "Option::is_none")]
    pub latest_version_uuid: Option<String>,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    #[serde(rename = "updatedAt")]
    pub updated_at: String,
    pub version: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ApplicationScaffoldPage {
    pub items: Vec<ApplicationScaffoldResponse>,
    pub total: i64,
    pub page: i32,
    pub page_size: i32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ApplicationScaffoldVersionResponse {
    pub id: String,
    #[serde(rename = "scaffoldUuid")]
    pub scaffold_uuid: String,
    /// The author-facing version string; `version` below is the row's
    /// optimistic-lock counter (same split as `BuildTemplateResponse`).
    #[serde(rename = "templateVersion")]
    pub template_version: String,
    pub changelog: String,
    #[serde(rename = "artifactUuid", skip_serializing_if = "Option::is_none")]
    pub artifact_uuid: Option<String>,
    #[serde(rename = "sourceAppVersion", skip_serializing_if = "Option::is_none")]
    pub source_app_version: Option<String>,
    #[serde(rename = "platformTargets")]
    pub platform_targets: Vec<String>,
    #[serde(rename = "packageSizeBytes")]
    pub package_size_bytes: String,
    #[serde(rename = "checksumSha256", skip_serializing_if = "Option::is_none")]
    pub checksum_sha256: Option<String>,
    pub status: String,
    #[serde(rename = "publishedAt", skip_serializing_if = "Option::is_none")]
    pub published_at: Option<String>,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    #[serde(rename = "updatedAt")]
    pub updated_at: String,
    pub version: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ApplicationScaffoldVersionPage {
    pub items: Vec<ApplicationScaffoldVersionResponse>,
    pub total: i64,
    pub page: i32,
    pub page_size: i32,
}

// -- requests -----------------------------------------------------------------

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CreateApplicationScaffoldRequest {
    #[serde(rename = "appUuid")]
    pub app_uuid: String,
    #[serde(rename = "templateType", default)]
    pub template_type: Option<String>,
    #[serde(rename = "categoryUuid")]
    pub category_uuid: String,
    #[serde(rename = "templateKey")]
    pub template_key: String,
    #[serde(rename = "displayName")]
    pub display_name: String,
    pub summary: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(rename = "visibility", default)]
    pub visibility: Option<String>,
    #[serde(rename = "pricingModel", default)]
    pub pricing_model: Option<String>,
    #[serde(rename = "priceMinor", default)]
    pub price_minor: Option<String>,
    #[serde(rename = "currency", default)]
    pub currency: Option<String>,
    #[serde(rename = "iconMediaRef", default)]
    pub icon_media_ref: Option<String>,
    #[serde(rename = "coverMediaRef", default)]
    pub cover_media_ref: Option<String>,
    #[serde(default)]
    pub screenshots: Vec<ApplicationScaffoldScreenshot>,
    #[serde(rename = "initialVersion", default)]
    pub initial_version: Option<CreateApplicationScaffoldVersionRequest>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct UpdateApplicationScaffoldRequest {
    #[serde(rename = "displayName", default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(rename = "categoryUuid", default)]
    pub category_uuid: Option<String>,
    #[serde(default)]
    pub visibility: Option<String>,
    #[serde(rename = "pricingModel", default)]
    pub pricing_model: Option<String>,
    #[serde(rename = "priceMinor", default)]
    pub price_minor: Option<String>,
    #[serde(default)]
    pub currency: Option<String>,
    #[serde(rename = "iconMediaRef", default)]
    pub icon_media_ref: Option<String>,
    #[serde(rename = "coverMediaRef", default)]
    pub cover_media_ref: Option<String>,
    /// `None` preserves the current screenshots; `Some([])` clears them.
    #[serde(default)]
    pub screenshots: Option<Vec<ApplicationScaffoldScreenshot>>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CreateApplicationScaffoldVersionRequest {
    pub version: String,
    #[serde(default)]
    pub changelog: Option<String>,
    #[serde(rename = "artifactUuid", default)]
    pub artifact_uuid: Option<String>,
    #[serde(rename = "sourceAppVersion", default)]
    pub source_app_version: Option<String>,
    #[serde(rename = "platformTargets", default)]
    pub platform_targets: Option<Vec<String>>,
    #[serde(rename = "packageSizeBytes", default)]
    pub package_size_bytes: Option<String>,
    #[serde(rename = "checksumSha256", default)]
    pub checksum_sha256: Option<String>,
}

// -- admin requests -----------------------------------------------------------

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CreateScaffoldCategoryRequest {
    #[serde(rename = "parentId", default)]
    pub parent_id: Option<String>,
    #[serde(rename = "categoryKey")]
    pub category_key: String,
    #[serde(rename = "displayName")]
    pub display_name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(rename = "sortOrder", default)]
    pub sort_order: Option<i32>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct UpdateScaffoldCategoryRequest {
    #[serde(rename = "parentId", default)]
    pub parent_id: Option<String>,
    #[serde(rename = "displayName", default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(rename = "sortOrder", default)]
    pub sort_order: Option<i32>,
    #[serde(default)]
    pub status: Option<String>,
}

/// Backend moderation body. The author surface cannot set these fields; the
/// backend PATCH is the only path that moves `status` / `review_note` /
/// `is_featured`.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct UpdateApplicationScaffoldAdminRequest {
    #[serde(default)]
    pub status: Option<String>,
    #[serde(rename = "reviewNote", default)]
    pub review_note: Option<String>,
    #[serde(rename = "isFeatured", default)]
    pub is_featured: Option<bool>,
    #[serde(default)]
    pub visibility: Option<String>,
}

// -- queries ------------------------------------------------------------------

/// `marketplaceTemplates.list` filters. The store always adds the tenant,
/// `PUBLIC`, and `PUBLISHED` predicates on top; these facets only narrow.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ListMarketplaceScaffoldsQuery {
    #[serde(default = "crate::dto::default_page")]
    pub page: i32,
    #[serde(default = "crate::dto::default_page_size")]
    pub page_size: i32,
    #[serde(default)]
    pub keyword: Option<String>,
    #[serde(default)]
    pub category_uuid: Option<String>,
    #[serde(default)]
    pub pricing_model: Option<String>,
    #[serde(default)]
    pub template_type: Option<String>,
    #[serde(default)]
    pub sort: Option<String>,
}

/// `appTemplates.list` filters (the caller's own listings).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ListApplicationScaffoldsQuery {
    #[serde(default = "crate::dto::default_page")]
    pub page: i32,
    #[serde(default = "crate::dto::default_page_size")]
    pub page_size: i32,
    #[serde(default)]
    pub keyword: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
}

/// Backend `appTemplates.list` filters (tenant-wide, every status).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ListApplicationScaffoldsAdminQuery {
    #[serde(default = "crate::dto::default_page")]
    pub page: i32,
    #[serde(default = "crate::dto::default_page_size")]
    pub page_size: i32,
    #[serde(default)]
    pub keyword: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub category_uuid: Option<String>,
    #[serde(default)]
    pub visibility: Option<String>,
    #[serde(default)]
    pub template_type: Option<String>,
}
