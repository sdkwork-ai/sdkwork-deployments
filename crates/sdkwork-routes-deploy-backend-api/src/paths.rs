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

// -- app template marketplace (admin) ------------------------------------------

pub const TEMPLATE_CATEGORIES: &str = "/backend/v3/api/template_categories";
pub const TEMPLATE_CATEGORY: &str = "/backend/v3/api/template_categories/{categoryUuid}";
pub const APP_TEMPLATES: &str = "/backend/v3/api/app_templates";
pub const APP_TEMPLATE: &str = "/backend/v3/api/app_templates/{templateUuid}";
pub const APP_TEMPLATE_VERSIONS: &str = "/backend/v3/api/app_templates/{templateUuid}/versions";
pub const APP_TEMPLATE_VERSION: &str =
    "/backend/v3/api/app_templates/{templateUuid}/versions/{versionUuid}";
