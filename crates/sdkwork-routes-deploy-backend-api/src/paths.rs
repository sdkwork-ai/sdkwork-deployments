pub const PREFIX: &str = "/backend/v3/api";

pub const NGINX_CONFIGS: &str = "/backend/v3/api/nginx/configs";
pub const NGINX_CONFIG: &str = "/backend/v3/api/nginx/configs/{configId}";
pub const NGINX_CONFIG_VALIDATE: &str = "/backend/v3/api/nginx/configs/{configId}/validate";
pub const NGINX_CONFIG_DEPLOY: &str = "/backend/v3/api/nginx/configs/{configId}/deploy";
pub const NGINX_RELOAD: &str = "/backend/v3/api/nginx/reload";
pub const NGINX_STATUS: &str = "/backend/v3/api/nginx/status";
pub const SERVERS: &str = "/backend/v3/api/servers";
pub const SERVER: &str = "/backend/v3/api/servers/{serverId}";
pub const NODE_CLUSTERS: &str = "/backend/v3/api/node_clusters";
pub const NODE_CLUSTER: &str = "/backend/v3/api/node_clusters/{clusterId}";
pub const AUDIT_LOGS: &str = "/backend/v3/api/audit_logs";
pub const ENTITLEMENTS: &str = "/backend/v3/api/entitlements";
pub const BUILD_QUEUE: &str = "/backend/v3/api/build_queue";
pub const RUNNERS: &str = "/backend/v3/api/runners";
pub const TLS_ACCOUNTS: &str = "/backend/v3/api/tls/accounts";
pub const TLS_ORDERS: &str = "/backend/v3/api/tls/orders";
pub const TLS_ORDER_ADVANCE: &str = "/backend/v3/api/tls/orders/{orderId}/advance";
pub const TLS_ORDER_FAIL: &str = "/backend/v3/api/tls/orders/{orderId}/fail";
pub const TLS_ORDER_CHALLENGE_RESULT: &str =
    "/backend/v3/api/tls/orders/{orderId}/challenge_result";
pub const TLS_ORDER_VERSIONS: &str = "/backend/v3/api/tls/orders/{orderId}/versions";
pub const TLS_ORDER_CHALLENGES: &str = "/backend/v3/api/tls/orders/{orderId}/challenges";
pub const CERTIFICATE_ORDERS: &str = "/backend/v3/api/certificates/{certificateId}/orders";
pub const RETENTION_RUN: &str = "/backend/v3/api/retention/run";
pub const USAGE_INGEST: &str = "/backend/v3/api/usage/ingest";
pub const USAGE_RECONCILE: &str = "/backend/v3/api/usage/reconcile";
pub const SIGNING_IDENTITY_HEALTH: &str = "/backend/v3/api/signing_identity_health";
pub const SOURCE_EVENTS: &str = "/backend/v3/api/source_events";

// -- application ledger (operator) --------------------------------------------

/// The operator application ledger: every app in the operator's tenant (or
/// across tenants when the context carries none), filterable by the same
/// ownership and endpoint facets as the tenant surface.
pub const APPS: &str = "/backend/v3/api/apps";

/// The factory reconciliation report (section 9): dangling cross-module
/// references for the operator's tenant. Read-only; never repairs.
pub const APP_FACTORY_RECONCILIATION: &str = "/backend/v3/api/apps/factory_reconciliation";

// -- application identity (operator retry entry) ------------------------------

/// Re-runs IAM installation provisioning for one application.
///
/// `apps.create` is the only path that ever provisions, so an application whose
/// create ran while IAM was unreachable had no way back. This is that way back;
/// it is an operator (backend) action, not a tenant-console one.
pub const APP_IDENTITY_PROVISION: &str = "/backend/v3/api/apps/{appId}/identity_provision";

// -- application scaffolds (admin) --------------------------------------------

pub const SCAFFOLD_CATEGORIES: &str = "/backend/v3/api/scaffold_categories";
pub const SCAFFOLD_CATEGORY: &str = "/backend/v3/api/scaffold_categories/{categoryUuid}";
pub const SCAFFOLDS: &str = "/backend/v3/api/scaffolds";
pub const SCAFFOLD: &str = "/backend/v3/api/scaffolds/{scaffoldUuid}";
pub const SCAFFOLD_VERSIONS: &str = "/backend/v3/api/scaffolds/{scaffoldUuid}/versions";
pub const SCAFFOLD_VERSION: &str =
    "/backend/v3/api/scaffolds/{scaffoldUuid}/versions/{versionUuid}";

// -- download catalog (admin) --------------------------------------------------

pub const DOWNLOADS_CATALOGS: &str = "/backend/v3/api/downloads/catalogs";
pub const DOWNLOADS_CATALOG: &str = "/backend/v3/api/downloads/catalogs/{catalogKey}";
pub const DOWNLOADS_CATALOG_PUBLISH: &str =
    "/backend/v3/api/downloads/catalogs/{catalogKey}/publish";
pub const DOWNLOADS_CARD: &str = "/backend/v3/api/downloads/catalogs/{catalogKey}/cards/{cardKey}";
pub const DOWNLOADS_ITEM: &str =
    "/backend/v3/api/downloads/catalogs/{catalogKey}/cards/{cardKey}/items/{itemKey}";
pub const DOWNLOADS_UPDATE: &str =
    "/backend/v3/api/downloads/catalogs/{catalogKey}/cards/{cardKey}/items/{itemKey}/update";
