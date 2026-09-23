//! Traffic usage metering contract shared by the Web Server data plane and
//! the Deploy control plane.
//!
//! The Web Server records per-domain / per-server-IP traffic facts
//! (`traffic.requests`, `traffic.ingress_bytes`, `traffic.egress_bytes`)
//! attributed to the serving tenant and app; the control plane ingests them
//! into `deploy_usage_event` (deduplicated) and rolls them up into the daily
//! billing tables.

use serde::{Deserialize, Serialize};

/// Traffic usage dimensions recorded by the Web Server data plane.
pub const USAGE_DIMENSION_TRAFFIC_REQUESTS: &str = "traffic.requests";
pub const USAGE_DIMENSION_TRAFFIC_INGRESS_BYTES: &str = "traffic.ingress_bytes";
pub const USAGE_DIMENSION_TRAFFIC_EGRESS_BYTES: &str = "traffic.egress_bytes";

/// Traffic attribution recorded with every usage event: the serving domain,
/// the server's local IP/port, and — when the request was served through the
/// Deploy control plane — the app identity and app/binding references.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageEventAttribution {
    /// Normalized request hostname (domain dimension).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hostname: Option<String>,
    /// Local server IP that served the request (server dimension).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_ip: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_port: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub listener_id: Option<String>,
    /// App public uuid (`deploy_app.uuid`) when attributable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_id: Option<String>,
    /// App slug when attributable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_slug: Option<String>,
    /// Site public uuid (`deploy_app.uuid`) when attributable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_uuid: Option<String>,
    /// Binding public uuid (`deploy_app_binding.uuid`) when attributable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub binding_uuid: Option<String>,
    /// Response status class (`2xx`, `3xx`, `4xx`, `5xx`) when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status_class: Option<String>,
}

/// One traffic usage event submitted by a Web Server node.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageEventIngestItem {
    /// Event tenant when the node could attribute it; `0` means unmanaged
    /// (the control plane resolves the tenant from the binding when
    /// possible).
    #[serde(rename = "tenantId", default)]
    pub tenant_id: i64,
    #[serde(rename = "organizationId", default)]
    pub organization_id: i64,
    /// Site public uuid when attributable; resolved to `app_id` by the
    /// control plane.
    #[serde(rename = "appUuid", default, skip_serializing_if = "Option::is_none")]
    pub app_uuid: Option<String>,
    /// Binding public uuid when attributable; resolved to `binding_id` and
    /// used for tenant attribution by the control plane.
    #[serde(
        rename = "bindingUuid",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub binding_uuid: Option<String>,
    /// Aggregation window start (RFC 3339).
    #[serde(rename = "periodStart")]
    pub period_start: String,
    /// Usage dimension (`traffic.requests`, `traffic.ingress_bytes`,
    /// `traffic.egress_bytes`).
    pub dimension: String,
    /// Aggregated quantity over the window.
    pub quantity: i64,
    pub unit: String,
    /// Idempotency key (`traffic:<window>:<tenant>:<app>:<binding>:<host>:<ip>:<dim>`).
    #[serde(rename = "deduplicationKey")]
    pub deduplication_key: String,
    /// Traffic attribution (domain, server IP, app, status class).
    #[serde(rename = "attribution", default)]
    pub attribution: UsageEventAttribution,
    /// When the events were observed on the node (RFC 3339).
    #[serde(rename = "observedAt")]
    pub observed_at: String,
}

/// Batch traffic usage ingest request from a Web Server node.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IngestUsageEventsRequest {
    /// Node identity for audit (`SDKWORK_WEBSERVER_NODE_UUID`).
    #[serde(rename = "nodeUuid", default, skip_serializing_if = "Option::is_none")]
    pub node_uuid: Option<String>,
    pub events: Vec<UsageEventIngestItem>,
}

/// Result of a batch ingest.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageIngestResult {
    #[serde(rename = "ingested")]
    pub ingested: usize,
    #[serde(rename = "duplicates")]
    pub duplicates: usize,
    #[serde(rename = "rejected")]
    pub rejected: usize,
}

// ---------------------------------------------------------------------------
// Aggregated traffic usage statistics (read model)
// ---------------------------------------------------------------------------
//
// Consumed by the Web Server's operations surface, which shares this database
// and reads the append-only facts through
// `DeployRepository::traffic_usage_statistics_lookup`.
//
// These are **in-process** read models, not wire DTOs: the quantities stay
// `i64` deliberately. The decimal-string encoding the API contract requires
// (API_SPEC §13.6) belongs to the consuming surface's own response types, where
// one serde rule can cover the whole document — encoding it here would put a
// wire concern inside an internal port and make the two representations drift.
//
// Every view aggregates the **facts** (`deploy_usage_event`) rather than the
// daily rollups (`deploy_tenant_usage_daily` / `deploy_app_usage_daily`).
// Reading the facts keeps one authority: totals, the daily series, and the
// per-app breakdown are then equal to each other by construction instead of by
// the reconciliation job having run most recently.

/// One usage dimension's aggregate over the requested window.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrafficUsageTotal {
    /// Usage dimension (`traffic.requests`, `traffic.ingress_bytes`,
    /// `traffic.egress_bytes`).
    pub dimension: String,
    /// Sum of the dimension's quantity over the window.
    pub quantity: i64,
    /// Unit of `quantity` (`REQUEST`, `BYTE`).
    pub unit: String,
}

/// One day of one usage dimension, for trend series.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrafficUsageDailyPoint {
    /// Calendar day in UTC (`YYYY-MM-DD`).
    #[serde(rename = "usageDate")]
    pub usage_date: String,
    pub dimension: String,
    pub quantity: i64,
}

/// One app's aggregate of one usage dimension over the window.
///
/// A row whose [`app_uuid`](Self::app_uuid) is absent is the **unattributed
/// bucket**: traffic the edge served for a hostname it could not resolve to an
/// app (or a tenant-less window). It is reported rather than dropped so the
/// per-app rows always sum back to the corresponding total.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrafficUsageAppTotal {
    #[serde(rename = "appUuid", default, skip_serializing_if = "Option::is_none")]
    pub app_uuid: Option<String>,
    #[serde(rename = "appSlug", default, skip_serializing_if = "Option::is_none")]
    pub app_slug: Option<String>,
    pub dimension: String,
    pub quantity: i64,
    pub unit: String,
}

/// One tenant's aggregate of one usage dimension over the window.
///
/// Only populated for a platform-wide read (`tenant_id: None`); a tenant-scoped
/// read returns an empty list because the answer would be the caller's own
/// totals repeated once per dimension.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrafficUsageTenantTotal {
    #[serde(rename = "tenantId")]
    pub tenant_id: i64,
    pub dimension: String,
    pub quantity: i64,
    pub unit: String,
}

/// Aggregate traffic usage over a closed date window.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrafficUsageStatistics {
    /// Inclusive UTC day the window starts on (`YYYY-MM-DD`).
    #[serde(rename = "dateFrom")]
    pub date_from: String,
    /// **Exclusive** UTC day the window ends on (`YYYY-MM-DD`): the facts
    /// selected are `date_from <= day < date_to`. Half-open on purpose, so
    /// consecutive windows neither double-count nor drop a day.
    #[serde(rename = "dateTo")]
    pub date_to: String,
    /// Window totals, one per dimension present in the window.
    pub totals: Vec<TrafficUsageTotal>,
    /// Daily series, one row per (day, dimension) present in the window.
    pub daily: Vec<TrafficUsageDailyPoint>,
    /// Per-app breakdown of the top apps by total traffic, plus the
    /// unattributed bucket when it carries traffic.
    pub apps: Vec<TrafficUsageAppTotal>,
    /// Per-tenant breakdown; empty for a tenant-scoped read.
    pub tenants: Vec<TrafficUsageTenantTotal>,
    /// Whether the read covered every tenant rather than one. Reported so a
    /// surface cannot render a platform-wide number as if it were the
    /// caller's own (or the reverse).
    #[serde(rename = "platformScope")]
    pub platform_scope: bool,
}

/// Filters for [`TrafficUsageStatistics`].
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrafficUsageStatisticsQuery {
    /// Inclusive UTC day (`YYYY-MM-DD`).
    #[serde(rename = "dateFrom")]
    pub date_from: String,
    /// Exclusive UTC day (`YYYY-MM-DD`).
    #[serde(rename = "dateTo")]
    pub date_to: String,
    /// Restrict to one dimension; `None` returns every dimension.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dimension: Option<String>,
    /// Size of the per-app breakdown. The unattributed bucket is always kept
    /// in addition to this bound, because dropping it would make the breakdown
    /// silently disagree with the totals.
    #[serde(rename = "topApps")]
    pub top_apps: i64,
}
