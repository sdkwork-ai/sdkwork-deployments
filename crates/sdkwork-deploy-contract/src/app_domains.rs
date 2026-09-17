//! App publishing domain contract shared by the Deploy control plane and the
//! Web Server fallback resolver.
//!
//! The control plane provisions every app's default publishable hostnames
//! (`<slug>.app[-<env>].<suffix>`, `sdkwork-deploy-core::app_domains`) and
//! resolves unmatched Web Server hosts to a compiled app revision descriptor
//! (`ResolvedDeployServer`).

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// One resolved Deploy server for a Web Server fallback lookup: the app
/// whose active binding owns the requested hostname, together with its
/// latest compiled website runtime descriptor (`deploy_app_revision`).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ResolvedDeployServer {
    /// The owning app's public uuid.
    #[serde(rename = "appUuid")]
    pub app_uuid: String,
    /// The app's slug.
    #[serde(rename = "appSlug")]
    pub app_slug: String,
    /// The matched binding hostname (normalized lowercase ASCII).
    #[serde(rename = "hostname")]
    pub hostname: String,
    /// The binding's path prefix (`/` for default app bindings).
    #[serde(rename = "pathPrefix")]
    pub path_prefix: String,
    /// Binding action: `SERVE` or `REDIRECT`.
    #[serde(rename = "actionType")]
    pub action_type: String,
    /// The owning tenant (usage metering attribution).
    #[serde(rename = "tenantId")]
    pub tenant_id: i64,
    /// The owning app's public uuid (usage metering attribution).
    #[serde(rename = "appId", default, skip_serializing_if = "Option::is_none")]
    pub app_id: Option<String>,
    /// The matched binding's public uuid (per-domain usage attribution).
    #[serde(rename = "bindingId", default, skip_serializing_if = "Option::is_none")]
    pub binding_id: Option<String>,
    /// The compiled `sdkwork.website-runtime.descriptor` document
    /// (`deploy_app_revision.descriptor_json`), which the Web Server
    /// activates as a fallback app.
    #[serde(rename = "descriptorJson")]
    pub descriptor_json: Value,
    /// SHA-256 of the compiled descriptor.
    #[serde(rename = "descriptorSha256")]
    pub descriptor_sha256: String,
    /// The descriptor's revision number in the app's revision chain.
    #[serde(rename = "revisionNo")]
    pub revision_no: i64,
    /// Lifecycle environment of the binding
    /// (`development|test|staging|demo|production`).
    #[serde(rename = "environment")]
    pub environment: String,
    /// The app's nginx-compatible configuration document for this hostname,
    /// resolved from `deploy_nginx_config` (environment-scoped row first, then
    /// the app-level `deploy_app.nginx_conf` base). `None` when the app has no
    /// managed nginx configuration, in which case the Web Server keeps serving
    /// through the compiled descriptor alone.
    #[serde(rename = "nginxConf", default, skip_serializing_if = "Option::is_none")]
    pub nginx_conf: Option<String>,
    /// SHA-256 of [`Self::nginx_conf`] when present.
    #[serde(
        rename = "nginxConfSha256",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub nginx_conf_sha256: Option<String>,
    /// The app-domain prefix this app publishes under
    /// (`<appDomainLabel>.app[-<env>].<suffix>`); the slug unless the app
    /// declares a custom prefix.
    #[serde(rename = "appDomainLabel")]
    pub app_domain_label: String,
}

/// Result of idempotently provisioning an app's default publishing domains:
/// platform DNS zones, EXACT `deploy_domain` rows (auto-verified because the
/// platform owns the apex domains) and `deploy_app_binding` rows.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ProvisionAppDomainsResult {
    #[serde(rename = "createdZones")]
    pub created_zones: usize,
    #[serde(rename = "createdDomains")]
    pub created_domains: usize,
    #[serde(rename = "existingDomains")]
    pub existing_domains: usize,
    #[serde(rename = "createdBindings")]
    pub created_bindings: usize,
    #[serde(rename = "existingBindings")]
    pub existing_bindings: usize,
    /// Every provisioned hostname (`<slug>.app[-<env>].<suffix>`).
    #[serde(rename = "hostnames")]
    pub hostnames: Vec<String>,
}
