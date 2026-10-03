//! App template marketplace catalog wire contracts.
//!
//! An author publishes a `deploy_app` as a template listing (`AppTemplate`),
//! optionally with versions (`AppTemplateVersion`); the marketplace browses the
//! tenant's `PUBLIC` + `PUBLISHED` listings. This module owns the catalog only:
//! categories, listings and versions. Acquisition, payment and entitlement are
//! owned by the sdkwork-order order center and sdkwork-payment, so no purchase,
//! order or entitlement contract lives here. Everything is tenant-scoped: the
//! marketplace browse surface only ever sees the caller's tenant's listings.

use serde::{Deserialize, Serialize};

/// Wire vocabulary for `deploy_app_template.visibility`.
pub const TEMPLATE_VISIBILITY_PUBLIC: &str = "PUBLIC";
pub const TEMPLATE_VISIBILITY_PRIVATE: &str = "PRIVATE";

/// Wire vocabulary for `deploy_app_template.pricing_model`.
pub const TEMPLATE_PRICING_FREE: &str = "FREE";
pub const TEMPLATE_PRICING_PAID: &str = "PAID";

/// Wire vocabulary for `deploy_app_template.status`.
pub const TEMPLATE_STATUS_DRAFT: &str = "DRAFT";
pub const TEMPLATE_STATUS_PENDING_REVIEW: &str = "PENDING_REVIEW";
pub const TEMPLATE_STATUS_PUBLISHED: &str = "PUBLISHED";
pub const TEMPLATE_STATUS_REJECTED: &str = "REJECTED";
pub const TEMPLATE_STATUS_DISABLED: &str = "DISABLED";

/// Wire vocabulary for `deploy_app_template_version.status`.
pub const TEMPLATE_VERSION_STATUS_DRAFT: &str = "DRAFT";
pub const TEMPLATE_VERSION_STATUS_PUBLISHED: &str = "PUBLISHED";
pub const TEMPLATE_VERSION_STATUS_WITHDRAWN: &str = "WITHDRAWN";

/// Wire vocabulary for `deploy_app_template_category.status`.
pub const TEMPLATE_CATEGORY_STATUS_ACTIVE: &str = "ACTIVE";
pub const TEMPLATE_CATEGORY_STATUS_DISABLED: &str = "DISABLED";

/// Wire vocabulary for `deploy_app_template.template_type`: the artifact
/// form a template was conversationally created as (project-file authoring).
/// Extending the vocabulary is a baseline CHECK + contract enum change.
pub const TEMPLATE_TYPE_APP: &str = "APP";
pub const TEMPLATE_TYPE_PPT: &str = "PPT";
pub const TEMPLATE_TYPE_VIDEO: &str = "VIDEO";

/// Marketplace browse ordering. `POPULAR` ranks by install count, `NEWEST`
/// (the default) by recency.
pub const MARKETPLACE_SORT_NEWEST: &str = "NEWEST";
pub const MARKETPLACE_SORT_POPULAR: &str = "POPULAR";

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct TemplateCategoryResponse {
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
pub struct TemplateCategoryPage {
    pub items: Vec<TemplateCategoryResponse>,
    pub total: i64,
    pub page: i32,
    pub page_size: i32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AppTemplateSummaryResponse {
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
pub struct AppTemplateSummaryPage {
    pub items: Vec<AppTemplateSummaryResponse>,
    pub total: i64,
    pub page: i32,
    pub page_size: i32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AppTemplateResponse {
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
pub struct AppTemplatePage {
    pub items: Vec<AppTemplateResponse>,
    pub total: i64,
    pub page: i32,
    pub page_size: i32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AppTemplateVersionResponse {
    pub id: String,
    #[serde(rename = "templateUuid")]
    pub template_uuid: String,
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
pub struct AppTemplateVersionPage {
    pub items: Vec<AppTemplateVersionResponse>,
    pub total: i64,
    pub page: i32,
    pub page_size: i32,
}

// -- requests -----------------------------------------------------------------

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CreateAppTemplateRequest {
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
    #[serde(rename = "initialVersion", default)]
    pub initial_version: Option<CreateAppTemplateVersionRequest>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct UpdateAppTemplateRequest {
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
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CreateAppTemplateVersionRequest {
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
pub struct CreateTemplateCategoryRequest {
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
pub struct UpdateTemplateCategoryRequest {
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
pub struct UpdateAppTemplateAdminRequest {
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
pub struct ListMarketplaceTemplatesQuery {
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
pub struct ListAppTemplatesQuery {
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
pub struct ListAppTemplatesAdminQuery {
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
