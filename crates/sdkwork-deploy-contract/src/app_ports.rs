use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::app_composition::{AppCompositionResponse, UpdateAppCompositionRequest};
use crate::app_delivery::*;
use crate::app_resource_link::*;
use crate::app_source::*;
use crate::app_source_spec::*;
use crate::dto::*;
use crate::problem::{DeployServiceError, DeployServiceResult};
use crate::usage::{IngestUsageEventsRequest, UsageIngestResult};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeployAppRequestContext {
    pub tenant_id: i64,
    pub actor_id: Option<i64>,
    pub organization_id: Option<i64>,
    pub session_id: Option<String>,
    /// The deploy application the caller's credential proves it is
    /// (`APPLICATION_IDENTITY_SPEC.md` §4: an access token carries the deploy
    /// application in `app_id`/`aud`, and `aud` must equal `app_id`).
    ///
    /// Comes from the verified principal, never from a request body or path, so a
    /// client cannot widen it. `None` means the credential names no application —
    /// a user auth token without an `app_id` claim — and leaves the pre-existing
    /// tenant/owner boundary in force. Data-plane reads of application-scoped
    /// rows must be narrowed by this value while it is present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_id: Option<String>,
    /// Permission codes the caller's credential actually carries.
    ///
    /// The route manifest's required permission gates the surface; the approval
    /// step needs a second, narrower grant (`deploy.apps.approve`) that the
    /// framework cannot express as a route permission because it depends on the
    /// request body (`approve: true`). Projected from the verified principal, so
    /// a client cannot invent it.
    #[serde(default, skip_serializing)]
    pub permissions: Vec<String>,
    #[serde(default, skip_serializing)]
    pub auth_token: Option<String>,
    #[serde(default, skip_serializing)]
    pub access_token: Option<String>,
}

impl DeployAppRequestContext {
    /// The deploy application identity the caller's credential proves, if any.
    ///
    /// Normalizes a blank claim to `None`: an empty `app_id` cannot name an
    /// application, and treating it as a value would build a filter that matches
    /// no row while reading as if scope had been applied.
    pub fn app_scope(&self) -> Option<&str> {
        self.app_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
    }

    /// Whether the credential carries `required`, honoring the `*` wildcard
    /// families (`deploy.*`, `deploy.apps.*`) the IAM scope vocabulary uses.
    pub fn has_permission(&self, required: &str) -> bool {
        self.permissions
            .iter()
            .any(|granted| permission_matches(granted, required))
    }

    /// The reach this caller has over rows another user owns.
    ///
    /// Derived from the verified principal — the actor for audit and
    /// [`crate::permissions::PERM_MANAGE_ALL_OWNERSHIP`] for the lift — so a
    /// resource query can gate ownership without the service having to re-derive
    /// the decision at every call site.
    pub fn ownership_reach(&self) -> crate::permissions::OwnershipReach {
        crate::permissions::OwnershipReach::from_context(self)
    }

    /// Gate for an environment promotion that records an approval.
    ///
    /// `deploy.apps.write` lets an operator promote; recording an approval is a
    /// second, narrower right because it asserts another party's sign-off. The
    /// check is fail-closed: a credential with no `deploy.apps.approve` grant
    /// cannot promote with `approve: true`, even if it can promote otherwise.
    pub fn require_promotion_approval(&self) -> DeployServiceResult<()> {
        if self.has_permission(DEPLOY_APPS_APPROVE_PERMISSION) {
            Ok(())
        } else {
            Err(DeployServiceError::forbidden(
                "deploy.apps.approve permission is required to approve an environment promotion",
            ))
        }
    }
}

/// Permission code that authorizes recording an approval on a promotion.
pub const DEPLOY_APPS_APPROVE_PERMISSION: &str = "deploy.apps.approve";

fn permission_matches(granted: &str, required: &str) -> bool {
    let granted = granted.trim();
    if granted.is_empty() {
        return false;
    }
    if granted == "*" || granted == required {
        return true;
    }
    if let Some(prefix) = granted.strip_suffix(".*") {
        return required == prefix || required.starts_with(&format!("{prefix}."));
    }
    // A bare trailing `*` is a prefix grant (`deploy.apps*`).
    if let Some(prefix) = granted.strip_suffix('*') {
        return required.starts_with(prefix);
    }
    false
}

#[cfg(test)]
mod permission_tests {
    use super::*;

    fn context(permissions: &[&str]) -> DeployAppRequestContext {
        DeployAppRequestContext {
            tenant_id: 7,
            actor_id: Some(11),
            permissions: permissions
                .iter()
                .map(|value| (*value).to_owned())
                .collect(),
            ..Default::default()
        }
    }

    #[test]
    fn exact_and_wildcard_grants_match() {
        for grant in ["deploy.apps.approve", "deploy.*", "deploy.apps.*", "*"] {
            assert!(
                context(&[grant]).has_permission(DEPLOY_APPS_APPROVE_PERMISSION),
                "{grant} must authorize the approval gate"
            );
        }
        for grant in [
            "deploy.apps.write",
            "deploy.apps.approvals",
            "deployx.*",
            "",
        ] {
            assert!(
                !context(&[grant]).has_permission(DEPLOY_APPS_APPROVE_PERMISSION),
                "{grant} must not authorize the approval gate"
            );
        }
    }

    #[test]
    fn promotion_approval_is_fail_closed() {
        assert!(context(&[]).require_promotion_approval().is_err());
        assert!(
            matches!(
                context(&["deploy.apps.write"]).require_promotion_approval(),
                Err(DeployServiceError::Forbidden(_))
            ),
            "a promote-only credential must not record an approval"
        );
        assert!(context(&["deploy.apps.approve"])
            .require_promotion_approval()
            .is_ok());
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeployBackendRequestContext {
    pub operator_id: Option<i64>,
    pub tenant_id: Option<i64>,
}

/// Traffic usage event list filters (per-domain / per-server-IP / per-app).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct UsageEventQuery {
    #[serde(default = "crate::dto::default_page")]
    pub page: i32,
    #[serde(default = "crate::dto::default_page_size")]
    pub page_size: i32,
    /// Filter by binding public uuid (per-domain attribution).
    #[serde(rename = "bindingId", default, skip_serializing_if = "Option::is_none")]
    pub binding_id: Option<String>,
    /// Filter by usage dimension (`traffic.requests`, …).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dimension: Option<String>,
    /// Filter by hostname (attribution hostname exact match).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hostname: Option<String>,
    /// Filter by server IP (attribution serverIp exact match).
    #[serde(rename = "serverIp", default, skip_serializing_if = "Option::is_none")]
    pub server_ip: Option<String>,
    /// Filter by app public uuid (attribution appId exact match).
    #[serde(rename = "appId", default, skip_serializing_if = "Option::is_none")]
    pub app_id: Option<String>,
    /// Window start inclusive (RFC 3339).
    #[serde(rename = "since", default, skip_serializing_if = "Option::is_none")]
    pub since: Option<String>,
    /// Window start exclusive (RFC 3339).
    #[serde(rename = "until", default, skip_serializing_if = "Option::is_none")]
    pub until: Option<String>,
}

/// `apps.list` filters.
///
/// This struct existed but was never used: the route deserialized the shared
/// `PageQuery` instead, so the declared filters were unreachable and the
/// ownership facet had nowhere to live. It is now the real query type for the
/// route.
///
/// Two declared fields were dropped in the same change: `app_type: Option<i32>`
/// and `status: Option<i32>`. Both were dead, and `status` contradicted the
/// canonical vocabulary — `AppStatus` is a SCREAMING string enum
/// (`DRAFT`/`ACTIVE`/`PAUSED`/`ARCHIVED`, which is also what the DDL CHECK
/// accepts), so an integer status could only ever have produced filters the
/// database rejects. Nothing deserialized this struct, and it is not declared in
/// the published contract, so removing them is invisible on the wire.
///
/// Every member here is a **facet, not a grant**: omitted widens the answer to
/// everything the caller can already reach, and setting one can only narrow
/// that set. Reach itself is decided by the owner gate in the repository and is
/// always conjunctive with these filters.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ListAppsQuery {
    #[serde(default = "crate::dto::default_page")]
    pub page: i32,
    #[serde(default = "crate::dto::default_page_size")]
    pub page_size: i32,
    /// Case-insensitive match on the app name or slug.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keyword: Option<String>,
    /// Deprecated alias of [`Self::ownership_scope`], kept because the first
    /// published version of this filter used the shorter name. A request that
    /// sets both must set them to the same value; `ownership_scope` is what new
    /// callers send.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<crate::app_delivery::AppOwnerType>,
    /// Narrow the listing to one ownership mode: `TENANT`, `ORGANIZATION`, or
    /// `USER` (`PLATFORM` is reachable for an operator that can already read
    /// platform applications).
    ///
    /// This is a **facet, not a grant**: omitting it widens the answer to every
    /// mode the caller can already reach, and setting it can only narrow that
    /// set. Reach itself is decided by the owner gate in the repository, which
    /// is conjunctive with this filter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ownership_scope: Option<crate::app_delivery::AppOwnerType>,
    /// Narrow the listing to one ownership subject, interpreted as the
    /// ownership mode's own pointer:
    ///
    /// - absent: no organization filter;
    /// - `0`: the **tenant** mode (`owner_type = 'TENANT'`);
    /// - positive: the **organization** mode in exactly that organization
    ///   (`owner_type = 'ORGANIZATION' AND organization_id = n`).
    ///
    /// The parameter is `owner_organization_id`, **not** `organization_id`: the
    /// Web Framework's client-context guard (`API_SPEC.md` §10, catalog B12)
    /// rejects a client-supplied `organization_id`/`org_id` query key outright,
    /// because those name the *ambient request context* and must come from the
    /// verified principal. This filter names a row attribute, so it needs a
    /// name the guard does not reserve. The response field is still
    /// `organizationId` — that direction is a value, not a selector.
    ///
    /// The value is a decimal int64 string (`API_SPEC.md` §13.6). Negative,
    /// non-numeric, and leading-plus values are rejected rather than silently
    /// matching nothing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_organization_id: Option<String>,
    /// Narrow the listing to one or more endpoint kinds (`app_kind`).
    ///
    /// Not deserialized by serde: a repeated query key (`app_kind=A&app_kind=B`)
    /// is not expressible through `serde_urlencoded`'s struct deserializer, so
    /// the route parses the raw query pairs and fills this field. The wire
    /// contract declares it as `style: form, explode: true`, which the
    /// generated SDKs serialize as repeated keys; a single comma-separated
    /// value is accepted too. Empty means "every kind".
    #[serde(skip)]
    pub app_kind: Vec<crate::app_delivery::AppKind>,
}

impl ListAppsQuery {
    /// The effective ownership facet, merging the deprecated `scope` alias.
    ///
    /// A request that names two different modes is a client bug and is rejected
    /// instead of silently picking one.
    pub fn resolved_ownership_scope(
        &self,
    ) -> DeployServiceResult<Option<crate::app_delivery::AppOwnerType>> {
        match (self.scope, self.ownership_scope) {
            (Some(alias), Some(scope)) if alias != scope => Err(DeployServiceError::validation(
                "scope and ownership_scope must name the same ownership mode".to_owned(),
            )),
            (Some(alias), _) => Ok(Some(alias)),
            (None, scope) => Ok(scope),
        }
    }

    /// Parses [`Self::owner_organization_id`] into the ownership pointer it
    /// names.
    ///
    /// See the field documentation for the `0`/positive split. Returns `None`
    /// for an absent or blank value, matching every other optional query
    /// string on this surface.
    pub fn organization_filter(&self) -> DeployServiceResult<Option<i64>> {
        let Some(raw) = self
            .owner_organization_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            return Ok(None);
        };
        // `i64::from_str` accepts a leading `+`, which the contract's
        // `^-?[0-9]+$` pattern does not declare; rejecting it keeps the query
        // value and the OpenAPI pattern saying the same thing.
        if raw.starts_with('+') {
            return Err(DeployServiceError::validation(
                "owner_organization_id must be 0 (tenant) or a positive organization id".to_owned(),
            ));
        }
        let parsed: i64 = raw.parse().map_err(|_| {
            DeployServiceError::validation(format!(
                "owner_organization_id {raw} is not an int64 organization id"
            ))
        })?;
        if parsed < 0 {
            return Err(DeployServiceError::validation(
                "owner_organization_id must be 0 (tenant) or a positive organization id".to_owned(),
            ));
        }
        Ok(Some(parsed))
    }

    /// The `app_kind` values as the wire strings the repository binds.
    pub fn app_kind_values(&self) -> Vec<&'static str> {
        self.app_kind.iter().map(|kind| kind.as_str()).collect()
    }
}

/// `apps.list` on the operator (backend) surface.
///
/// Same facets as [`ListAppsQuery`], plus the one thing only an operator view
/// needs: the tenant to look at. The fields are duplicated rather than flattened
/// because `serde_urlencoded` does not deserialize a struct through `flatten`;
/// [`Self::filter`] is the single place the mapping back to [`ListAppsQuery`]
/// lives.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ListAppsAdminQuery {
    #[serde(default = "crate::dto::default_page")]
    pub page: i32,
    #[serde(default = "crate::dto::default_page_size")]
    pub page_size: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keyword: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<crate::app_delivery::AppOwnerType>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ownership_scope: Option<crate::app_delivery::AppOwnerType>,
    /// See [`ListAppsQuery::owner_organization_id`]; named the same on both
    /// surfaces so one client shape works for each.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_organization_id: Option<String>,
    /// Restrict the ledger to one tenant. Absent means the tenant scope of the
    /// operator's own request context, or — for a platform operator whose
    /// context carries no tenant — every tenant.
    ///
    /// This is **not** an app-api parameter. A tenant member cannot widen their
    /// reach by naming a tenant: the app-api route never reads this field, and
    /// the backend route binds the effective tenant inside the permission-
    /// guarded operator surface.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenant_id: Option<String>,
    /// See [`ListAppsQuery::app_kind`]; filled from the raw query by the route.
    #[serde(skip)]
    pub app_kind: Vec<crate::app_delivery::AppKind>,
}

impl ListAppsAdminQuery {
    /// The shared facet set, with the admin-only tenant filter removed.
    pub fn filter(&self) -> ListAppsQuery {
        ListAppsQuery {
            page: self.page,
            page_size: self.page_size,
            keyword: self.keyword.clone(),
            scope: self.scope,
            ownership_scope: self.ownership_scope,
            owner_organization_id: self.owner_organization_id.clone(),
            app_kind: self.app_kind.clone(),
        }
    }

    /// Parses [`Self::tenant_id`] into the tenant the ledger is restricted to.
    pub fn tenant_filter(&self) -> DeployServiceResult<Option<i64>> {
        parse_optional_subject_id(self.tenant_id.as_deref(), "tenant_id")
    }
}

/// Parses an optional decimal int64 subject id, rejecting the shapes an id can
/// never have.
fn parse_optional_subject_id(value: Option<&str>, field: &str) -> DeployServiceResult<Option<i64>> {
    let Some(raw) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    let parsed: i64 = raw.parse().map_err(|_| {
        DeployServiceError::validation(format!("{field} {raw} is not an int64 subject id"))
    })?;
    if parsed <= 0 {
        return Err(DeployServiceError::validation(format!(
            "{field} must be a positive subject id"
        )));
    }
    Ok(Some(parsed))
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ListDomainZonesQuery {
    #[serde(default = "crate::dto::default_page")]
    pub page: i32,
    #[serde(default = "crate::dto::default_page_size")]
    pub page_size: i32,
    pub status: Option<String>,
    pub keyword: Option<String>,
    /// Restrict the inventory to one ownership kind (`USER` / `PLATFORM`) — the
    /// vocabulary [`crate::dto::ZoneScope`] names.
    ///
    /// `None` keeps both, which is the audit view; the console passes `USER` so
    /// the platform-provisioned `app.<suffix>` zones do not appear as root
    /// domains the operator owns. Those platform zones are edge infrastructure
    /// rather than anybody's root domain, while a listing that omits the scope
    /// still sees both — which is what the certificate coverage picker needs so
    /// it can reach them.
    ///
    /// A zone's scope is derived from its `user_id` (`None` ⇒ `Platform`), so
    /// [`crate::dto::ZoneScope::for_owner`] is the one place that mapping lives.
    pub scope: Option<crate::dto::ZoneScope>,
    /// Restrict the inventory to the zones pinned to one cloud account.
    ///
    /// Three states, which is why this is one optional string rather than an id
    /// plus a flag: an absent member filters nothing, the literal
    /// [`ZONE_PROVIDER_ACCOUNT_UNASSIGNED`] returns the zones pinned to no account
    /// at all — the ones whose account resolves per operation — and anything else
    /// names the account to match.
    ///
    /// This is a **facet, not a grant**, exactly like `scope`: it can only narrow
    /// the set the owner gate already allows, so a caller cannot use it to learn
    /// that another user's zone is pinned to an account. An account id that no
    /// reachable zone carries simply answers empty.
    ///
    /// The literal cannot collide with a real account id: the column's own shape
    /// rule (`chk_deploy_dns_zone_provider_account`) admits only ASCII
    /// alphanumerics and `_.:-` after a leading alphanumeric, so a value containing
    /// whitespace is unreachable as an id.
    pub provider_account_id: Option<String>,
}

/// The wire value that asks for zones bound to no cloud account.
///
/// See [`ZoneProviderAccountFilter::from_query`] for why this is a reserved
/// literal rather than a second boolean parameter.
pub const ZONE_PROVIDER_ACCOUNT_UNASSIGNED: &str = "UNASSIGNED";

/// Query of a Zone's synced resolution records.
///
/// `hostname_id` restricts the page to one registered hostname — the rows the
/// sync matched to that hostname, wildcard semantics already applied at sync
/// time. An absent member pages the whole Zone snapshot, which is what the
/// Zone-level read asks for.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ListDomainDnsRecordsQuery {
    #[serde(default = "crate::dto::default_page")]
    pub page: i32,
    #[serde(default = "crate::dto::default_page_size")]
    pub page_size: i32,
    #[serde(rename = "hostname_id", default)]
    pub hostname_id: Option<String>,
    /// 主机记录 keyword filter (substring, case-insensitive).
    #[serde(rename = "host", default)]
    pub host: Option<String>,
    /// Exact record-type filter (`A`, `AAAA`, `CNAME`, `TXT`, `MX`).
    #[serde(rename = "record_type", default)]
    pub record_type: Option<String>,
}

/// The cloud-account pin a zone listing is restricted to.
///
/// The three states are mutually exclusive by construction rather than by
/// convention, which is what keeps "no filter", "pinned to no account" and
/// "pinned to this account" from being expressible as contradictory pairs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ZoneProviderAccountFilter {
    /// Return every zone the caller may reach, whatever it is pinned to.
    Any,
    /// Return only the zones pinned to no account.
    Unassigned,
    /// Return only the zones pinned to this account.
    Assigned(String),
}

impl ZoneProviderAccountFilter {
    /// Reads the filter off the raw query value.
    ///
    /// A blank value is an omission, matching every other optional query string on
    /// this surface: `?provider_account_id=` is what a cleared form field submits,
    /// and reading it as "the account whose id is the empty string" would turn a
    /// cleared filter into an empty list.
    ///
    /// The reserved literal is matched case-sensitively on purpose. Account ids are
    /// case-sensitive, so folding case here would make `unassigned` mean something
    /// the caller's own account id `unassigned` does not.
    pub fn from_query(value: Option<&str>) -> Self {
        match value.map(str::trim) {
            None | Some("") => Self::Any,
            Some(ZONE_PROVIDER_ACCOUNT_UNASSIGNED) => Self::Unassigned,
            Some(account_id) => Self::Assigned(account_id.to_owned()),
        }
    }
}

impl ListDomainZonesQuery {
    /// Resolves the query's pin member into the filter the store reads.
    ///
    /// A method rather than a field on the query so the wire shape stays a plain
    /// optional string — the three states exist for the store's benefit, and putting
    /// them on the deserialized struct would mean the query could hold a state the
    /// wire cannot express.
    pub fn provider_account_filter(&self) -> ZoneProviderAccountFilter {
        ZoneProviderAccountFilter::from_query(self.provider_account_id.as_deref())
    }
}

#[async_trait]
pub trait DeployAppApi: Send + Sync {
    async fn list_domain_zones(
        &self,
        _context: &DeployAppRequestContext,
        _query: &ListDomainZonesQuery,
    ) -> DeployServiceResult<DomainZonePage> {
        Err(crate::DeployServiceError::Internal(
            "domain zone API is not implemented".to_owned(),
        ))
    }

    async fn create_domain_zone(
        &self,
        _context: &DeployAppRequestContext,
        _request: &CreateDomainZoneRequest,
    ) -> DeployServiceResult<DomainZoneResponse> {
        Err(crate::DeployServiceError::Internal(
            "domain zone API is not implemented".to_owned(),
        ))
    }

    async fn retrieve_domain_zone(
        &self,
        _context: &DeployAppRequestContext,
        _zone_id: &str,
    ) -> DeployServiceResult<DomainZoneResponse> {
        Err(crate::DeployServiceError::Internal(
            "domain zone API is not implemented".to_owned(),
        ))
    }

    async fn update_domain_zone(
        &self,
        _context: &DeployAppRequestContext,
        _zone_id: &str,
        _request: &UpdateDomainZoneRequest,
    ) -> DeployServiceResult<DomainZoneResponse> {
        Err(crate::DeployServiceError::Internal(
            "domain zone API is not implemented".to_owned(),
        ))
    }

    async fn delete_domain_zone(
        &self,
        _context: &DeployAppRequestContext,
        _zone_id: &str,
    ) -> DeployServiceResult<()> {
        Err(crate::DeployServiceError::Internal(
            "domain zone API is not implemented".to_owned(),
        ))
    }

    async fn list_domain_zone_verification_files(
        &self,
        _context: &DeployAppRequestContext,
        _zone_id: &str,
    ) -> DeployServiceResult<DomainVerificationFilePage> {
        Err(crate::DeployServiceError::Internal(
            "domain zone API is not implemented".to_owned(),
        ))
    }

    async fn create_domain_zone_verification_file(
        &self,
        _context: &DeployAppRequestContext,
        _zone_id: &str,
        _request: &CreateDomainVerificationFileRequest,
    ) -> DeployServiceResult<DomainVerificationFileResponse> {
        Err(crate::DeployServiceError::Internal(
            "domain zone API is not implemented".to_owned(),
        ))
    }

    async fn delete_domain_zone_verification_file(
        &self,
        _context: &DeployAppRequestContext,
        _zone_id: &str,
        _file_id: &str,
    ) -> DeployServiceResult<()> {
        Err(crate::DeployServiceError::Internal(
            "domain zone API is not implemented".to_owned(),
        ))
    }

    async fn check_domain_zone_verification_file(
        &self,
        _context: &DeployAppRequestContext,
        _zone_id: &str,
        _file_id: &str,
    ) -> DeployServiceResult<DomainVerificationFileCheckResponse> {
        Err(crate::DeployServiceError::Internal(
            "domain zone API is not implemented".to_owned(),
        ))
    }

    async fn list_domain_hostnames(
        &self,
        _context: &DeployAppRequestContext,
        _zone_id: &str,
        _page: i32,
        _page_size: i32,
    ) -> DeployServiceResult<DomainHostnamePage> {
        Err(crate::DeployServiceError::Internal(
            "domain hostname API is not implemented".to_owned(),
        ))
    }

    async fn create_domain_hostname(
        &self,
        _context: &DeployAppRequestContext,
        _zone_id: &str,
        _request: &CreateDomainHostnameRequest,
    ) -> DeployServiceResult<DomainHostnameResponse> {
        Err(crate::DeployServiceError::Internal(
            "domain hostname API is not implemented".to_owned(),
        ))
    }

    async fn retrieve_domain_hostname(
        &self,
        _context: &DeployAppRequestContext,
        _zone_id: &str,
        _hostname_id: &str,
    ) -> DeployServiceResult<DomainHostnameResponse> {
        Err(crate::DeployServiceError::Internal(
            "domain hostname API is not implemented".to_owned(),
        ))
    }

    async fn delete_domain_hostname(
        &self,
        _context: &DeployAppRequestContext,
        _zone_id: &str,
        _hostname_id: &str,
    ) -> DeployServiceResult<()> {
        Err(crate::DeployServiceError::Internal(
            "domain hostname API is not implemented".to_owned(),
        ))
    }

    async fn update_domain_hostname(
        &self,
        _context: &DeployAppRequestContext,
        _zone_id: &str,
        _hostname_id: &str,
        _request: &UpdateDomainHostnameRequest,
    ) -> DeployServiceResult<DomainHostnameResponse> {
        Err(crate::DeployServiceError::Internal(
            "domain hostname API is not implemented".to_owned(),
        ))
    }

    async fn verify_domain_hostname(
        &self,
        _context: &DeployAppRequestContext,
        _zone_id: &str,
        _hostname_id: &str,
    ) -> DeployServiceResult<DomainVerifyResponse> {
        Err(crate::DeployServiceError::Internal(
            "domain hostname verification API is not implemented".to_owned(),
        ))
    }

    /// Pages the Zone's synced DNS resolution records, optionally restricted
    /// to one hostname. The read is over the last cloud-account sync's
    /// snapshot: it never contacts the provider, so a page render costs a
    /// store read and never a vendor round trip.
    async fn list_domain_zone_dns_records(
        &self,
        _context: &DeployAppRequestContext,
        _zone_id: &str,
        _query: &ListDomainDnsRecordsQuery,
    ) -> DeployServiceResult<DomainDnsRecordPage> {
        Err(crate::DeployServiceError::Internal(
            "domain dns records API is not implemented".to_owned(),
        ))
    }

    /// Re-reads the Zone's record inventory through the same presenter chain
    /// issuance uses (the zone's pin, then the account center, then the
    /// deployment-level configuration) and replaces the stored snapshot.
    async fn sync_domain_zone_dns_records(
        &self,
        _context: &DeployAppRequestContext,
        _zone_id: &str,
    ) -> DeployServiceResult<DomainDnsSyncResponse> {
        Err(crate::DeployServiceError::Internal(
            "domain dns record sync API is not implemented".to_owned(),
        ))
    }

    /// Creates one resolution record of any managed type through the Zone's
    /// cloud account and joins it to the stored snapshot. The value is
    /// validated per type before the vendor is asked; the answer is the row
    /// as the provider now holds it.
    async fn create_domain_zone_dns_record(
        &self,
        _context: &DeployAppRequestContext,
        _zone_id: &str,
        _request: &CreateDomainDnsRecordRequest,
    ) -> DeployServiceResult<DomainDnsRecordResponse> {
        Err(crate::DeployServiceError::Internal(
            "domain dns record create API is not implemented".to_owned(),
        ))
    }

    /// Replaces one record in place through the Zone's cloud account and
    /// updates the stored row.
    async fn update_domain_zone_dns_record(
        &self,
        _context: &DeployAppRequestContext,
        _zone_id: &str,
        _record_id: &str,
        _request: &UpdateDomainDnsRecordRequest,
    ) -> DeployServiceResult<DomainDnsRecordResponse> {
        Err(crate::DeployServiceError::Internal(
            "domain dns record update API is not implemented".to_owned(),
        ))
    }

    /// Deletes one record on the provider and removes the stored row.
    async fn delete_domain_zone_dns_record(
        &self,
        _context: &DeployAppRequestContext,
        _zone_id: &str,
        _record_id: &str,
    ) -> DeployServiceResult<()> {
        Err(crate::DeployServiceError::Internal(
            "domain dns record delete API is not implemented".to_owned(),
        ))
    }

    /// Pauses one record (暂停解析) or resumes it, on the provider first and
    /// then in the stored row.
    async fn set_domain_zone_dns_record_status(
        &self,
        _context: &DeployAppRequestContext,
        _zone_id: &str,
        _record_id: &str,
        _request: &DomainDnsRecordStatusRequest,
    ) -> DeployServiceResult<DomainDnsRecordResponse> {
        Err(crate::DeployServiceError::Internal(
            "domain dns record status API is not implemented".to_owned(),
        ))
    }

    /// Declares whatever is missing from `request.hostnames` and advances each
    /// one's ownership proof once, so a certificate order can be submitted in a
    /// single round trip instead of `N` declarations plus `N` checks.
    ///
    /// This deliberately does **not** weaken ADR-20260723 §2: a certificate is
    /// still only issuable over hostnames that are already `VERIFIED`, and the
    /// caller cannot assert success — the response reports what the DNS lookup
    /// actually observed.
    async fn ensure_domain_hostname_claims(
        &self,
        _context: &DeployAppRequestContext,
        _zone_id: &str,
        _request: &EnsureDomainHostnameClaimsRequest,
    ) -> DeployServiceResult<DomainHostnameClaimBatchResponse> {
        Err(crate::DeployServiceError::Internal(
            "domain hostname claim API is not implemented".to_owned(),
        ))
    }

    /// Accounts the caller may pick when configuring DNS automation.
    ///
    /// Reads the IAM provider account center through the Deploy cloud account port,
    /// so the same account is usable from wherever it is convenient to manage it.
    /// This is also the "先判断是否已存在" probe behind both the zone and the
    /// certificate form: filtered by `dnsProvider` it answers whether the credential
    /// inputs can be skipped in favour of pinning an account that already exists, and
    /// the returned order is the server's own resolution precedence.
    async fn list_cloud_accounts(
        &self,
        _context: &DeployAppRequestContext,
        _query: &ListCloudAccountsQuery,
    ) -> DeployServiceResult<CloudAccountPage> {
        Err(crate::DeployServiceError::Internal(
            "cloud account API is not implemented".to_owned(),
        ))
    }

    /// Registers an account from console input, reusing one that already matches.
    async fn create_cloud_account(
        &self,
        _context: &DeployAppRequestContext,
        _request: &CreateCloudAccountRequest,
    ) -> DeployServiceResult<CloudAccountRegistrationResponse> {
        Err(crate::DeployServiceError::Internal(
            "cloud account API is not implemented".to_owned(),
        ))
    }

    async fn update_app_composition(
        &self,
        _context: &DeployAppRequestContext,
        _app_id: &str,
        _expected_app_version: i64,
        _idempotency_key: &str,
        _request: &UpdateAppCompositionRequest,
    ) -> DeployServiceResult<AppCompositionResponse> {
        Err(crate::DeployServiceError::Internal(
            "app composition API is not implemented".to_owned(),
        ))
    }

    async fn activate_app(
        &self,
        _context: &DeployAppRequestContext,
        _app_id: &str,
    ) -> DeployServiceResult<AppResponse> {
        Err(crate::DeployServiceError::Internal(
            "app activation API is not implemented".to_owned(),
        ))
    }

    async fn pause_app(
        &self,
        _context: &DeployAppRequestContext,
        _app_id: &str,
    ) -> DeployServiceResult<AppResponse> {
        Err(crate::DeployServiceError::Internal(
            "app pause API is not implemented".to_owned(),
        ))
    }

    /// Derived per-stage factory readiness (`APPLICATION_FACTORY_DEPLOYMENT_SPEC.md`
    /// section 11). The default answers "not implemented" so an older service host
    /// keeps compiling while the route exists.
    async fn app_factory_readiness(
        &self,
        _context: &DeployAppRequestContext,
        _app_id: &str,
    ) -> DeployServiceResult<AppFactoryReadiness> {
        Err(crate::DeployServiceError::Internal(
            "app factory readiness API is not implemented".to_owned(),
        ))
    }

    /// Forward-only uninstall (section 3 stage 6): disable the IAM installation,
    /// then archive the deploy environments and the application.
    async fn uninstall_app(
        &self,
        _context: &DeployAppRequestContext,
        _app_id: &str,
        _idempotency_key: &str,
    ) -> DeployServiceResult<AppUninstallResponse> {
        Err(crate::DeployServiceError::Internal(
            "app uninstall API is not implemented".to_owned(),
        ))
    }

    /// Reconciles the factory's soft cross-module references (section 9): the
    /// identity anchor, the hosting carrier, the installation domains, and the
    /// token anchor. Read-only; never repairs.
    async fn list_artifacts(
        &self,
        _context: &DeployAppRequestContext,
        _page: i32,
        _page_size: i32,
    ) -> DeployServiceResult<ArtifactPage> {
        Err(crate::DeployServiceError::Internal(
            "artifact API is not implemented".to_owned(),
        ))
    }

    async fn create_artifact(
        &self,
        _context: &DeployAppRequestContext,
        _request: &CreateArtifactRequest,
    ) -> DeployServiceResult<ArtifactResponse> {
        Err(crate::DeployServiceError::Internal(
            "artifact API is not implemented".to_owned(),
        ))
    }

    async fn retrieve_artifact(
        &self,
        _context: &DeployAppRequestContext,
        _artifact_id: &str,
    ) -> DeployServiceResult<ArtifactResponse> {
        Err(crate::DeployServiceError::Internal(
            "artifact API is not implemented".to_owned(),
        ))
    }

    async fn retain_artifact(
        &self,
        _context: &DeployAppRequestContext,
        _artifact_id: &str,
    ) -> DeployServiceResult<()> {
        Err(crate::DeployServiceError::Internal(
            "artifact API is not implemented".to_owned(),
        ))
    }

    async fn list_env_variables(
        &self,
        _context: &DeployAppRequestContext,
        _app_id: &str,
        _environment: Option<&str>,
    ) -> DeployServiceResult<EnvVariablePage> {
        Err(crate::DeployServiceError::Internal(
            "env variable API is not implemented".to_owned(),
        ))
    }

    async fn create_env_variable(
        &self,
        _context: &DeployAppRequestContext,
        _app_id: &str,
        _request: &CreateEnvVariableRequest,
    ) -> DeployServiceResult<EnvVariableResponse> {
        Err(crate::DeployServiceError::Internal(
            "env variable API is not implemented".to_owned(),
        ))
    }

    /// The account bindings of one application environment.
    ///
    /// `deploy_application_resource_link` is the authority for "which official
    /// account / mini program / payment account does this environment publish
    /// through"; `deploy_app_platform_target.app_id_value` is not a second
    /// authority (see `app_resource_link.rs`).
    async fn list_app_resource_links(
        &self,
        _context: &DeployAppRequestContext,
        _app_id: &str,
        _query: &ListAppResourceLinksQuery,
    ) -> DeployServiceResult<AppResourceLinkPage> {
        Err(crate::DeployServiceError::Internal(
            "app resource link API is not implemented".to_owned(),
        ))
    }

    /// Binds an account to one environment, or replaces the addressed binding.
    ///
    /// An explicit upsert, not a hidden create: the addressed key is
    /// `(environment, kind)` for the single-instance kinds and
    /// `(environment, kind, resourceRef)` for a payment account.
    async fn upsert_app_resource_link(
        &self,
        _context: &DeployAppRequestContext,
        _app_id: &str,
        _request: &UpsertAppResourceLinkRequest,
    ) -> DeployServiceResult<AppResourceLinkResponse> {
        Err(crate::DeployServiceError::Internal(
            "app resource link API is not implemented".to_owned(),
        ))
    }

    /// Soft-retires one binding. The row stays for audit; the runtime stops
    /// resolving it.
    async fn delete_app_resource_link(
        &self,
        _context: &DeployAppRequestContext,
        _app_id: &str,
        _resource_link_id: &str,
    ) -> DeployServiceResult<()> {
        Err(crate::DeployServiceError::Internal(
            "app resource link API is not implemented".to_owned(),
        ))
    }

    async fn list_certificates(
        &self,
        _context: &DeployAppRequestContext,
        _page: i32,
        _page_size: i32,
    ) -> DeployServiceResult<CertificatePage> {
        Err(crate::DeployServiceError::Internal(
            "certificate API is not implemented".to_owned(),
        ))
    }

    async fn create_certificate(
        &self,
        _context: &DeployAppRequestContext,
        _idempotency_key: &str,
        _request: &CreateCertificateRequest,
    ) -> DeployServiceResult<CertificateResponse> {
        Err(crate::DeployServiceError::Internal(
            "certificate API is not implemented".to_owned(),
        ))
    }

    async fn retrieve_certificate(
        &self,
        _context: &DeployAppRequestContext,
        _certificate_id: &str,
    ) -> DeployServiceResult<CertificateResponse> {
        Err(crate::DeployServiceError::Internal(
            "certificate API is not implemented".to_owned(),
        ))
    }

    async fn delete_certificate(
        &self,
        _context: &DeployAppRequestContext,
        _certificate_id: &str,
    ) -> DeployServiceResult<()> {
        Err(crate::DeployServiceError::Internal(
            "certificate API is not implemented".to_owned(),
        ))
    }

    async fn renew_certificate(
        &self,
        _context: &DeployAppRequestContext,
        _certificate_id: &str,
    ) -> DeployServiceResult<CertificateResponse> {
        Err(crate::DeployServiceError::Internal(
            "certificate API is not implemented".to_owned(),
        ))
    }

    async fn list_certificate_renewals(
        &self,
        _context: &DeployAppRequestContext,
        _certificate_id: &str,
        _page: i32,
        _page_size: i32,
    ) -> DeployServiceResult<CertificateRenewalPage> {
        Err(crate::DeployServiceError::Internal(
            "certificate renewal history is not implemented".to_owned(),
        ))
    }

    async fn list_health_checks(
        &self,
        _context: &DeployAppRequestContext,
        _app_id: &str,
    ) -> DeployServiceResult<HealthCheckPage> {
        Err(crate::DeployServiceError::Internal(
            "health check API is not implemented".to_owned(),
        ))
    }

    async fn create_health_check(
        &self,
        _context: &DeployAppRequestContext,
        _app_id: &str,
        _request: &CreateHealthCheckRequest,
    ) -> DeployServiceResult<HealthCheckResponse> {
        Err(crate::DeployServiceError::Internal(
            "health check API is not implemented".to_owned(),
        ))
    }

    async fn create_upload_session(
        &self,
        _context: &DeployAppRequestContext,
        _request: &CreateDeployUploadSessionRequest,
    ) -> DeployServiceResult<DeployUploadSessionResponse> {
        Err(crate::DeployServiceError::Internal(
            "upload session API is not implemented".to_owned(),
        ))
    }

    async fn retrieve_upload_session(
        &self,
        _context: &DeployAppRequestContext,
        _upload_session_id: &str,
    ) -> DeployServiceResult<DeployUploadSessionResponse> {
        Err(crate::DeployServiceError::Internal(
            "upload session API is not implemented".to_owned(),
        ))
    }

    async fn complete_upload_session(
        &self,
        _context: &DeployAppRequestContext,
        _upload_session_id: &str,
        _request: &CompleteDeployUploadSessionRequest,
    ) -> DeployServiceResult<DeployUploadSessionResponse> {
        Err(crate::DeployServiceError::Internal(
            "upload session API is not implemented".to_owned(),
        ))
    }

    async fn cancel_upload_session(
        &self,
        _context: &DeployAppRequestContext,
        _upload_session_id: &str,
    ) -> DeployServiceResult<DeployUploadSessionResponse> {
        Err(crate::DeployServiceError::Internal(
            "upload session API is not implemented".to_owned(),
        ))
    }

    async fn list_apps(
        &self,
        _context: &DeployAppRequestContext,
        _query: &ListAppsQuery,
    ) -> DeployServiceResult<AppPage> {
        Err(crate::DeployServiceError::Internal(
            "list_apps API is not implemented".to_owned(),
        ))
    }

    async fn create_app(
        &self,
        _context: &DeployAppRequestContext,
        _idempotency_key: Option<&str>,
        _request: &CreateAppRequest,
    ) -> DeployServiceResult<AppResponse> {
        Err(crate::DeployServiceError::Internal(
            "create_app API is not implemented".to_owned(),
        ))
    }

    async fn retrieve_app(
        &self,
        _context: &DeployAppRequestContext,
        _app_id: &str,
    ) -> DeployServiceResult<AppResponse> {
        Err(crate::DeployServiceError::Internal(
            "retrieve_app API is not implemented".to_owned(),
        ))
    }

    async fn update_app(
        &self,
        _context: &DeployAppRequestContext,
        _app_id: &str,
        _request: &UpdateAppRequest,
    ) -> DeployServiceResult<AppResponse> {
        Err(crate::DeployServiceError::Internal(
            "update_app API is not implemented".to_owned(),
        ))
    }

    async fn list_app_domains(
        &self,
        _context: &DeployAppRequestContext,
        _app_id: &str,
    ) -> DeployServiceResult<AppDomainPage> {
        Err(crate::DeployServiceError::Internal(
            "list_app_domains API is not implemented".to_owned(),
        ))
    }

    async fn create_platform_target(
        &self,
        _context: &DeployAppRequestContext,
        _app_id: &str,
        _request: &CreatePlatformTargetRequest,
    ) -> DeployServiceResult<PlatformTargetResponse> {
        Err(crate::DeployServiceError::Internal(
            "create_platform_target API is not implemented".to_owned(),
        ))
    }

    async fn list_platform_targets(
        &self,
        _context: &DeployAppRequestContext,
        _app_id: &str,
    ) -> DeployServiceResult<PlatformTargetPage> {
        Err(crate::DeployServiceError::Internal(
            "list_platform_targets API is not implemented".to_owned(),
        ))
    }

    async fn retrieve_platform_target(
        &self,
        _context: &DeployAppRequestContext,
        _app_id: &str,
        _target_id: &str,
    ) -> DeployServiceResult<PlatformTargetResponse> {
        Err(crate::DeployServiceError::Internal(
            "retrieve_platform_target API is not implemented".to_owned(),
        ))
    }

    async fn create_source_repository(
        &self,
        _context: &DeployAppRequestContext,
        _app_id: &str,
        _request: &CreateSourceRepositoryRequest,
    ) -> DeployServiceResult<SourceRepositoryResponse> {
        Err(crate::DeployServiceError::Internal(
            "create_source_repository API is not implemented".to_owned(),
        ))
    }

    async fn list_source_repositories(
        &self,
        _context: &DeployAppRequestContext,
        _app_id: &str,
    ) -> DeployServiceResult<SourceRepositoryPage> {
        Err(crate::DeployServiceError::Internal(
            "list_source_repositories API is not implemented".to_owned(),
        ))
    }

    async fn retrieve_source_repository(
        &self,
        _context: &DeployAppRequestContext,
        _app_id: &str,
        _repo_id: &str,
    ) -> DeployServiceResult<SourceRepositoryResponse> {
        Err(crate::DeployServiceError::Internal(
            "retrieve_source_repository API is not implemented".to_owned(),
        ))
    }

    async fn retrieve_app_source(
        &self,
        _context: &DeployAppRequestContext,
        _app_id: &str,
    ) -> DeployServiceResult<AppSourceResponse> {
        Err(crate::DeployServiceError::Internal(
            "retrieve_app_source API is not implemented".to_owned(),
        ))
    }

    async fn update_app_source(
        &self,
        _context: &DeployAppRequestContext,
        _app_id: &str,
        _request: &UpdateAppSourceRequest,
    ) -> DeployServiceResult<AppSourceResponse> {
        Err(crate::DeployServiceError::Internal(
            "update_app_source API is not implemented".to_owned(),
        ))
    }

    async fn delete_app_source(
        &self,
        _context: &DeployAppRequestContext,
        _app_id: &str,
    ) -> DeployServiceResult<()> {
        Err(crate::DeployServiceError::Internal(
            "delete_app_source API is not implemented".to_owned(),
        ))
    }

    /// Probes the configured Git repository and records the verdict.
    ///
    /// Answers the updated source resource: the console needs the resulting
    /// `readiness`/`readinessReason` as well as the recorded
    /// `repoStatus`/`lastErrorCode`, and a second `GET /source` would be a race
    /// against the next write.
    async fn validate_app_source(
        &self,
        _context: &DeployAppRequestContext,
        _app_id: &str,
    ) -> DeployServiceResult<AppSourceResponse> {
        Err(crate::DeployServiceError::Internal(
            "validate_app_source API is not implemented".to_owned(),
        ))
    }

    async fn retrieve_app_remix(
        &self,
        _context: &DeployAppRequestContext,
        _app_id: &str,
    ) -> DeployServiceResult<AppRemixResponse> {
        Err(crate::DeployServiceError::Internal(
            "retrieve_app_remix API is not implemented".to_owned(),
        ))
    }

    async fn update_app_remix(
        &self,
        _context: &DeployAppRequestContext,
        _app_id: &str,
        _request: &UpdateAppRemixRequest,
    ) -> DeployServiceResult<AppRemixResponse> {
        Err(crate::DeployServiceError::Internal(
            "update_app_remix API is not implemented".to_owned(),
        ))
    }

    async fn create_build_template(
        &self,
        _context: &DeployAppRequestContext,
        _request: &CreateBuildTemplateRequest,
    ) -> DeployServiceResult<BuildTemplateResponse> {
        Err(crate::DeployServiceError::Internal(
            "create_build_template API is not implemented".to_owned(),
        ))
    }

    async fn list_build_templates(
        &self,
        _context: &DeployAppRequestContext,
        _page: i32,
        _page_size: i32,
    ) -> DeployServiceResult<BuildTemplatePage> {
        Err(crate::DeployServiceError::Internal(
            "list_build_templates API is not implemented".to_owned(),
        ))
    }

    async fn retrieve_build_template(
        &self,
        _context: &DeployAppRequestContext,
        _template_id: &str,
    ) -> DeployServiceResult<BuildTemplateResponse> {
        Err(crate::DeployServiceError::Internal(
            "retrieve_build_template API is not implemented".to_owned(),
        ))
    }

    async fn create_build(
        &self,
        _context: &DeployAppRequestContext,
        _request: &CreateBuildRequest,
    ) -> DeployServiceResult<BuildResponse> {
        Err(crate::DeployServiceError::Internal(
            "create_build API is not implemented".to_owned(),
        ))
    }

    async fn list_builds(
        &self,
        _context: &DeployAppRequestContext,
        _app_id: &str,
        _page: i32,
        _page_size: i32,
    ) -> DeployServiceResult<BuildPage> {
        Err(crate::DeployServiceError::Internal(
            "list_builds API is not implemented".to_owned(),
        ))
    }

    async fn retrieve_build(
        &self,
        _context: &DeployAppRequestContext,
        _app_id: &str,
        _build_id: &str,
    ) -> DeployServiceResult<BuildResponse> {
        Err(crate::DeployServiceError::Internal(
            "retrieve_build API is not implemented".to_owned(),
        ))
    }

    async fn update_build_state(
        &self,
        _context: &DeployAppRequestContext,
        _app_id: &str,
        _build_id: &str,
        _request: &UpdateBuildStateRequest,
    ) -> DeployServiceResult<BuildResponse> {
        Err(crate::DeployServiceError::Internal(
            "update_build_state API is not implemented".to_owned(),
        ))
    }

    async fn register_package(
        &self,
        _context: &DeployAppRequestContext,
        _request: &RegisterPackageRequest,
    ) -> DeployServiceResult<PackageResponse> {
        Err(crate::DeployServiceError::Internal(
            "register_package API is not implemented".to_owned(),
        ))
    }

    async fn list_packages(
        &self,
        _context: &DeployAppRequestContext,
        _app_id: &str,
        _page: i32,
        _page_size: i32,
    ) -> DeployServiceResult<PackagePage> {
        Err(crate::DeployServiceError::Internal(
            "list_packages API is not implemented".to_owned(),
        ))
    }

    async fn retrieve_package(
        &self,
        _context: &DeployAppRequestContext,
        _app_id: &str,
        _package_id: &str,
    ) -> DeployServiceResult<PackageResponse> {
        Err(crate::DeployServiceError::Internal(
            "retrieve_package API is not implemented".to_owned(),
        ))
    }

    async fn create_app_release(
        &self,
        _context: &DeployAppRequestContext,
        _request: &CreateAppReleaseRequest,
    ) -> DeployServiceResult<AppReleaseResponse> {
        Err(crate::DeployServiceError::Internal(
            "create_app_release API is not implemented".to_owned(),
        ))
    }

    async fn list_app_releases(
        &self,
        _context: &DeployAppRequestContext,
        _app_id: &str,
        _page: i32,
        _page_size: i32,
    ) -> DeployServiceResult<AppReleasePage> {
        Err(crate::DeployServiceError::Internal(
            "list_app_releases API is not implemented".to_owned(),
        ))
    }

    async fn list_channel_releases(
        &self,
        _context: &DeployAppRequestContext,
        _app_id: &str,
        _environment: Option<&str>,
        _page: i32,
        _page_size: i32,
    ) -> DeployServiceResult<crate::ChannelReleasePage> {
        Err(crate::DeployServiceError::Internal(
            "list_channel_releases API is not implemented".to_owned(),
        ))
    }

    async fn retrieve_channel_release(
        &self,
        _context: &DeployAppRequestContext,
        _app_id: &str,
        _release_id: &str,
    ) -> DeployServiceResult<crate::ChannelReleaseResponse> {
        Err(crate::DeployServiceError::Internal(
            "retrieve_channel_release API is not implemented".to_owned(),
        ))
    }

    async fn create_channel_release(
        &self,
        _context: &DeployAppRequestContext,
        _app_id: &str,
        _request: &crate::CreateChannelReleaseRequest,
    ) -> DeployServiceResult<crate::ChannelReleaseResponse> {
        Err(crate::DeployServiceError::Internal(
            "create_channel_release API is not implemented".to_owned(),
        ))
    }

    async fn transition_channel_release(
        &self,
        _context: &DeployAppRequestContext,
        _app_id: &str,
        _release_id: &str,
        _request: &crate::TransitionChannelReleaseRequest,
    ) -> DeployServiceResult<crate::ChannelReleaseResponse> {
        Err(crate::DeployServiceError::Internal(
            "transition_channel_release API is not implemented".to_owned(),
        ))
    }

    async fn publish_channel_release(
        &self,
        _context: &DeployAppRequestContext,
        _app_id: &str,
        _release_id: &str,
        _request: &crate::ChannelReleasePublishCommandRequest,
    ) -> DeployServiceResult<crate::ChannelReleaseResponse> {
        Err(crate::DeployServiceError::Internal(
            "publish_channel_release API is not implemented".to_owned(),
        ))
    }

    async fn plan_channel_direct_upload(
        &self,
        _context: &DeployAppRequestContext,
        _app_id: &str,
        _request: &crate::CreateChannelDirectUploadPlanRequest,
    ) -> DeployServiceResult<crate::ChannelDirectUploadPlan> {
        Err(crate::DeployServiceError::Internal(
            "plan_channel_direct_upload API is not implemented".to_owned(),
        ))
    }

    async fn retrieve_app_release(
        &self,
        _context: &DeployAppRequestContext,
        _app_id: &str,
        _release_id: &str,
    ) -> DeployServiceResult<AppReleaseResponse> {
        Err(crate::DeployServiceError::Internal(
            "retrieve_app_release API is not implemented".to_owned(),
        ))
    }

    async fn update_app_release_status(
        &self,
        _context: &DeployAppRequestContext,
        _app_id: &str,
        _release_id: &str,
        _release_status: ReleaseStatus,
    ) -> DeployServiceResult<AppReleaseResponse> {
        Err(crate::DeployServiceError::Internal(
            "update_app_release_status API is not implemented".to_owned(),
        ))
    }

    async fn list_channels(
        &self,
        _context: &DeployAppRequestContext,
        _app_id: &str,
    ) -> DeployServiceResult<ChannelPage> {
        Err(crate::DeployServiceError::Internal(
            "list_channels API is not implemented".to_owned(),
        ))
    }

    async fn retrieve_channel(
        &self,
        _context: &DeployAppRequestContext,
        _app_id: &str,
        _channel_id: &str,
    ) -> DeployServiceResult<ChannelResponse> {
        Err(crate::DeployServiceError::Internal(
            "retrieve_channel API is not implemented".to_owned(),
        ))
    }

    async fn promote_channel(
        &self,
        _context: &DeployAppRequestContext,
        _app_id: &str,
        _channel_id: &str,
        _request: &PromoteChannelRequest,
    ) -> DeployServiceResult<ChannelRolloutResponse> {
        Err(crate::DeployServiceError::Internal(
            "promote_channel API is not implemented".to_owned(),
        ))
    }

    async fn list_channel_rollouts(
        &self,
        _context: &DeployAppRequestContext,
        _app_id: &str,
        _channel_id: &str,
        _page: i32,
        _page_size: i32,
    ) -> DeployServiceResult<ChannelRolloutPage> {
        Err(crate::DeployServiceError::Internal(
            "list_channel_rollouts API is not implemented".to_owned(),
        ))
    }

    async fn create_app_deployment(
        &self,
        _context: &DeployAppRequestContext,
        _request: &CreateAppDeploymentRequest,
    ) -> DeployServiceResult<AppDeploymentResponse> {
        Err(crate::DeployServiceError::Internal(
            "create_app_deployment API is not implemented".to_owned(),
        ))
    }

    async fn list_app_deployments(
        &self,
        _context: &DeployAppRequestContext,
        _app_id: &str,
        _page: i32,
        _page_size: i32,
    ) -> DeployServiceResult<AppDeploymentPage> {
        Err(crate::DeployServiceError::Internal(
            "list_app_deployments API is not implemented".to_owned(),
        ))
    }

    async fn retrieve_app_deployment(
        &self,
        _context: &DeployAppRequestContext,
        _app_id: &str,
        _deployment_id: &str,
    ) -> DeployServiceResult<AppDeploymentResponse> {
        Err(crate::DeployServiceError::Internal(
            "retrieve_app_deployment API is not implemented".to_owned(),
        ))
    }

    async fn create_signing_identity(
        &self,
        _context: &DeployAppRequestContext,
        _request: &CreateSigningIdentityRequest,
    ) -> DeployServiceResult<SigningIdentityResponse> {
        Err(crate::DeployServiceError::Internal(
            "create_signing_identity API is not implemented".to_owned(),
        ))
    }

    async fn list_signing_identities(
        &self,
        _context: &DeployAppRequestContext,
        _page: i32,
        _page_size: i32,
    ) -> DeployServiceResult<SigningIdentityPage> {
        Err(crate::DeployServiceError::Internal(
            "list_signing_identities API is not implemented".to_owned(),
        ))
    }

    async fn retrieve_signing_identity(
        &self,
        _context: &DeployAppRequestContext,
        _identity_id: &str,
    ) -> DeployServiceResult<SigningIdentityResponse> {
        Err(crate::DeployServiceError::Internal(
            "retrieve_signing_identity API is not implemented".to_owned(),
        ))
    }

    async fn list_usage_events(
        &self,
        _context: &DeployAppRequestContext,
        _query: &UsageEventQuery,
    ) -> DeployServiceResult<UsageEventPage> {
        Err(crate::DeployServiceError::Internal(
            "list_usage_events API is not implemented".to_owned(),
        ))
    }

    async fn create_app_database_profile(
        &self,
        _context: &DeployAppRequestContext,
        _app_id: &str,
        _request: &CreateAppDatabaseProfileRequest,
    ) -> DeployServiceResult<AppDatabaseProfileResponse> {
        Err(crate::DeployServiceError::Internal(
            "create_app_database_profile API is not implemented".to_owned(),
        ))
    }

    async fn list_app_database_profiles(
        &self,
        _context: &DeployAppRequestContext,
        _app_id: &str,
        _page: i32,
        _page_size: i32,
    ) -> DeployServiceResult<AppDatabaseProfilePage> {
        Err(crate::DeployServiceError::Internal(
            "list_app_database_profiles API is not implemented".to_owned(),
        ))
    }

    async fn retrieve_app_database_profile(
        &self,
        _context: &DeployAppRequestContext,
        _app_id: &str,
        _profile_id: &str,
    ) -> DeployServiceResult<AppDatabaseProfileResponse> {
        Err(crate::DeployServiceError::Internal(
            "retrieve_app_database_profile API is not implemented".to_owned(),
        ))
    }

    async fn update_app_database_profile(
        &self,
        _context: &DeployAppRequestContext,
        _app_id: &str,
        _profile_id: &str,
        _request: &UpdateAppDatabaseProfileRequest,
    ) -> DeployServiceResult<AppDatabaseProfileResponse> {
        Err(crate::DeployServiceError::Internal(
            "update_app_database_profile API is not implemented".to_owned(),
        ))
    }

    async fn create_app_database_migration(
        &self,
        _context: &DeployAppRequestContext,
        _app_id: &str,
        _profile_id: &str,
        _request: &CreateAppDatabaseMigrationRequest,
    ) -> DeployServiceResult<AppDatabaseMigrationResponse> {
        Err(crate::DeployServiceError::Internal(
            "create_app_database_migration API is not implemented".to_owned(),
        ))
    }

    async fn list_app_database_migrations(
        &self,
        _context: &DeployAppRequestContext,
        _app_id: &str,
        _profile_id: &str,
        _page: i32,
        _page_size: i32,
    ) -> DeployServiceResult<AppDatabaseMigrationPage> {
        Err(crate::DeployServiceError::Internal(
            "list_app_database_migrations API is not implemented".to_owned(),
        ))
    }

    async fn retrieve_app_database_migration(
        &self,
        _context: &DeployAppRequestContext,
        _app_id: &str,
        _profile_id: &str,
        _migration_id: &str,
    ) -> DeployServiceResult<AppDatabaseMigrationResponse> {
        Err(crate::DeployServiceError::Internal(
            "retrieve_app_database_migration API is not implemented".to_owned(),
        ))
    }

    async fn create_app_environment(
        &self,
        _context: &DeployAppRequestContext,
        _app_id: &str,
        _request: &CreateAppEnvironmentRequest,
    ) -> DeployServiceResult<AppEnvironmentResponse> {
        Err(crate::DeployServiceError::Internal(
            "create_app_environment API is not implemented".to_owned(),
        ))
    }

    async fn list_app_environments(
        &self,
        _context: &DeployAppRequestContext,
        _app_id: &str,
        _page: i32,
        _page_size: i32,
    ) -> DeployServiceResult<AppEnvironmentPage> {
        Err(crate::DeployServiceError::Internal(
            "list_app_environments API is not implemented".to_owned(),
        ))
    }

    async fn retrieve_app_environment(
        &self,
        _context: &DeployAppRequestContext,
        _app_id: &str,
        _environment_id: &str,
    ) -> DeployServiceResult<AppEnvironmentResponse> {
        Err(crate::DeployServiceError::Internal(
            "retrieve_app_environment API is not implemented".to_owned(),
        ))
    }

    async fn update_app_environment(
        &self,
        _context: &DeployAppRequestContext,
        _app_id: &str,
        _environment_id: &str,
        _request: &UpdateAppEnvironmentRequest,
    ) -> DeployServiceResult<AppEnvironmentResponse> {
        Err(crate::DeployServiceError::Internal(
            "update_app_environment API is not implemented".to_owned(),
        ))
    }

    async fn promote_environment(
        &self,
        _context: &DeployAppRequestContext,
        _app_id: &str,
        _environment_id: &str,
        _request: &PromoteEnvironmentRequest,
    ) -> DeployServiceResult<EnvironmentPromotionResponse> {
        Err(crate::DeployServiceError::Internal(
            "promote_environment API is not implemented".to_owned(),
        ))
    }

    async fn list_environment_promotions(
        &self,
        _context: &DeployAppRequestContext,
        _app_id: &str,
        _environment_id: &str,
        _page: i32,
        _page_size: i32,
    ) -> DeployServiceResult<EnvironmentPromotionPage> {
        Err(crate::DeployServiceError::Internal(
            "list_environment_promotions API is not implemented".to_owned(),
        ))
    }

    // -- source specs ---------------------------------------------------------

    async fn list_app_source_specs(
        &self,
        _context: &DeployAppRequestContext,
        _app_id: &str,
        _environment: Option<&str>,
    ) -> DeployServiceResult<AppSourceSpecPage> {
        Err(crate::DeployServiceError::Internal(
            "list_app_source_specs API is not implemented".to_owned(),
        ))
    }

    async fn create_app_source_spec(
        &self,
        _context: &DeployAppRequestContext,
        _app_id: &str,
        _idempotency_key: &str,
        _request: &CreateAppSourceSpecRequest,
    ) -> DeployServiceResult<AppSourceSpecResponse> {
        Err(crate::DeployServiceError::Internal(
            "create_app_source_spec API is not implemented".to_owned(),
        ))
    }

    async fn retrieve_app_source_spec(
        &self,
        _context: &DeployAppRequestContext,
        _app_id: &str,
        _spec_id: &str,
    ) -> DeployServiceResult<AppSourceSpecResponse> {
        Err(crate::DeployServiceError::Internal(
            "retrieve_app_source_spec API is not implemented".to_owned(),
        ))
    }

    async fn update_app_source_spec(
        &self,
        _context: &DeployAppRequestContext,
        _app_id: &str,
        _spec_id: &str,
        _expected_spec_version: i64,
        _request: &UpdateAppSourceSpecRequest,
    ) -> DeployServiceResult<AppSourceSpecResponse> {
        Err(crate::DeployServiceError::Internal(
            "update_app_source_spec API is not implemented".to_owned(),
        ))
    }

    async fn delete_app_source_spec(
        &self,
        _context: &DeployAppRequestContext,
        _app_id: &str,
        _spec_id: &str,
    ) -> DeployServiceResult<()> {
        Err(crate::DeployServiceError::Internal(
            "delete_app_source_spec API is not implemented".to_owned(),
        ))
    }

    async fn bind_app_source_spec_source(
        &self,
        _context: &DeployAppRequestContext,
        _app_id: &str,
        _spec_id: &str,
        _request: &BindAppSourceSpecSourceRequest,
    ) -> DeployServiceResult<AppSourceSpecResponse> {
        Err(crate::DeployServiceError::Internal(
            "bind_app_source_spec_source API is not implemented".to_owned(),
        ))
    }

    // -- app scaffold marketplace ---------------------------------------------

    async fn list_scaffold_categories(
        &self,
        _context: &DeployAppRequestContext,
        _include_disabled: bool,
    ) -> DeployServiceResult<crate::scaffold_market::ScaffoldCategoryPage> {
        Err(crate::DeployServiceError::Internal(
            "list_scaffold_categories API is not implemented".to_owned(),
        ))
    }

    async fn list_marketplace_scaffolds(
        &self,
        _context: &DeployAppRequestContext,
        _query: &crate::scaffold_market::ListMarketplaceScaffoldsQuery,
    ) -> DeployServiceResult<crate::scaffold_market::ApplicationScaffoldSummaryPage> {
        Err(crate::DeployServiceError::Internal(
            "list_marketplace_scaffolds API is not implemented".to_owned(),
        ))
    }

    async fn retrieve_marketplace_scaffold(
        &self,
        _context: &DeployAppRequestContext,
        _scaffold_uuid: &str,
    ) -> DeployServiceResult<crate::scaffold_market::ApplicationScaffoldResponse> {
        Err(crate::DeployServiceError::Internal(
            "retrieve_marketplace_scaffold API is not implemented".to_owned(),
        ))
    }

    async fn install_application_scaffold(
        &self,
        _context: &DeployAppRequestContext,
        _scaffold_uuid: &str,
        _idempotency_key: &str,
        _request: &crate::scaffold_market::InstallApplicationScaffoldRequest,
    ) -> DeployServiceResult<crate::scaffold_market::ApplicationScaffoldInstallResponse> {
        Err(crate::DeployServiceError::Internal(
            "install_application_scaffold API is not implemented".to_owned(),
        ))
    }

    async fn list_application_scaffolds(
        &self,
        _context: &DeployAppRequestContext,
        _query: &crate::scaffold_market::ListApplicationScaffoldsQuery,
    ) -> DeployServiceResult<crate::scaffold_market::ApplicationScaffoldPage> {
        Err(crate::DeployServiceError::Internal(
            "list_application_scaffolds API is not implemented".to_owned(),
        ))
    }

    async fn create_application_scaffold(
        &self,
        _context: &DeployAppRequestContext,
        _idempotency_key: &str,
        _request: &crate::scaffold_market::CreateApplicationScaffoldRequest,
    ) -> DeployServiceResult<crate::scaffold_market::ApplicationScaffoldResponse> {
        Err(crate::DeployServiceError::Internal(
            "create_application_scaffold API is not implemented".to_owned(),
        ))
    }

    async fn retrieve_application_scaffold(
        &self,
        _context: &DeployAppRequestContext,
        _scaffold_uuid: &str,
    ) -> DeployServiceResult<crate::scaffold_market::ApplicationScaffoldResponse> {
        Err(crate::DeployServiceError::Internal(
            "retrieve_application_scaffold API is not implemented".to_owned(),
        ))
    }

    async fn update_application_scaffold(
        &self,
        _context: &DeployAppRequestContext,
        _scaffold_uuid: &str,
        _request: &crate::scaffold_market::UpdateApplicationScaffoldRequest,
    ) -> DeployServiceResult<crate::scaffold_market::ApplicationScaffoldResponse> {
        Err(crate::DeployServiceError::Internal(
            "update_application_scaffold API is not implemented".to_owned(),
        ))
    }

    async fn delete_application_scaffold(
        &self,
        _context: &DeployAppRequestContext,
        _scaffold_uuid: &str,
    ) -> DeployServiceResult<()> {
        Err(crate::DeployServiceError::Internal(
            "delete_application_scaffold API is not implemented".to_owned(),
        ))
    }

    async fn submit_application_scaffold(
        &self,
        _context: &DeployAppRequestContext,
        _scaffold_uuid: &str,
    ) -> DeployServiceResult<crate::scaffold_market::ApplicationScaffoldResponse> {
        Err(crate::DeployServiceError::Internal(
            "submit_application_scaffold API is not implemented".to_owned(),
        ))
    }

    async fn list_application_scaffold_versions(
        &self,
        _context: &DeployAppRequestContext,
        _scaffold_uuid: &str,
        _page: i32,
        _page_size: i32,
    ) -> DeployServiceResult<crate::scaffold_market::ApplicationScaffoldVersionPage> {
        Err(crate::DeployServiceError::Internal(
            "list_application_scaffold_versions API is not implemented".to_owned(),
        ))
    }

    async fn create_application_scaffold_version(
        &self,
        _context: &DeployAppRequestContext,
        _scaffold_uuid: &str,
        _idempotency_key: &str,
        _request: &crate::scaffold_market::CreateApplicationScaffoldVersionRequest,
    ) -> DeployServiceResult<crate::scaffold_market::ApplicationScaffoldVersionResponse> {
        Err(crate::DeployServiceError::Internal(
            "create_application_scaffold_version API is not implemented".to_owned(),
        ))
    }
}

#[async_trait]
pub trait DeployBackendApi: Send + Sync {
    /// The operator application ledger: every app in the operator's tenant, or
    /// across tenants when the request context carries none, narrowed by the
    /// same ownership/endpoint facets the tenant surface offers.
    ///
    /// Unlike `DeployAppApi::list_apps` there is no owner gate: this surface is
    /// the operator's whole-ledger view, and its reach is bounded by the
    /// permission declared on the route plus the tenant scope below.
    async fn list_apps(
        &self,
        context: &DeployBackendRequestContext,
        query: &ListAppsAdminQuery,
    ) -> DeployServiceResult<AppPage>;

    async fn list_nginx_configs(
        &self,
        context: &DeployBackendRequestContext,
        query: &ListNginxConfigsQuery,
    ) -> DeployServiceResult<NginxConfigPage>;

    async fn create_nginx_config(
        &self,
        context: &DeployBackendRequestContext,
        request: &CreateNginxConfigRequest,
    ) -> DeployServiceResult<NginxConfigResponse>;

    async fn retrieve_nginx_config(
        &self,
        context: &DeployBackendRequestContext,
        config_id: &str,
    ) -> DeployServiceResult<NginxConfigResponse>;

    async fn update_nginx_config(
        &self,
        context: &DeployBackendRequestContext,
        config_id: &str,
        request: &UpdateNginxConfigRequest,
    ) -> DeployServiceResult<NginxConfigResponse>;

    async fn validate_nginx_config(
        &self,
        context: &DeployBackendRequestContext,
        config_id: &str,
    ) -> DeployServiceResult<NginxValidateResponse>;

    async fn deploy_nginx_config(
        &self,
        context: &DeployBackendRequestContext,
        config_id: &str,
    ) -> DeployServiceResult<NginxConfigResponse>;

    async fn reload_nginx(
        &self,
        context: &DeployBackendRequestContext,
    ) -> DeployServiceResult<NginxReloadResponse>;

    async fn retrieve_nginx_status(
        &self,
        context: &DeployBackendRequestContext,
    ) -> DeployServiceResult<NginxStatusResponse>;

    async fn list_servers(
        &self,
        context: &DeployBackendRequestContext,
        page: i32,
        page_size: i32,
        cluster_id: Option<String>,
    ) -> DeployServiceResult<ServerPage>;

    async fn create_server(
        &self,
        context: &DeployBackendRequestContext,
        request: &CreateServerRequest,
    ) -> DeployServiceResult<ServerResponse>;

    async fn update_server(
        &self,
        context: &DeployBackendRequestContext,
        server_id: &str,
        request: &UpdateServerRequest,
    ) -> DeployServiceResult<ServerResponse>;

    async fn list_node_clusters(
        &self,
        context: &DeployBackendRequestContext,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<NodeClusterPage>;

    async fn create_node_cluster(
        &self,
        context: &DeployBackendRequestContext,
        request: &CreateNodeClusterRequest,
    ) -> DeployServiceResult<NodeClusterResponse>;

    async fn update_node_cluster(
        &self,
        context: &DeployBackendRequestContext,
        cluster_id: &str,
        request: &UpdateNodeClusterRequest,
    ) -> DeployServiceResult<NodeClusterResponse>;

    async fn list_audit_logs(
        &self,
        context: &DeployBackendRequestContext,
        query: &AuditLogQuery,
        cursor: Option<&str>,
    ) -> DeployServiceResult<AuditLogPage>;

    async fn list_entitlement_projections(
        &self,
        context: &DeployBackendRequestContext,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<EntitlementProjectionPage>;

    async fn list_build_queue(
        &self,
        context: &DeployBackendRequestContext,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<BuildQueuePage>;

    async fn list_runner_health(
        &self,
        context: &DeployBackendRequestContext,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<RunnerHealthPage>;

    async fn create_acme_account(
        &self,
        context: &DeployBackendRequestContext,
        request: &CreateAcmeAccountRequest,
    ) -> DeployServiceResult<AcmeAccountResponse>;

    async fn list_acme_accounts(
        &self,
        context: &DeployBackendRequestContext,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<AcmeAccountPage>;

    async fn request_certificate_order(
        &self,
        context: &DeployBackendRequestContext,
        request: &RequestCertificateOrderRequest,
    ) -> DeployServiceResult<CertificateOrderResponse>;

    async fn advance_certificate_order(
        &self,
        context: &DeployBackendRequestContext,
        order_id: &str,
    ) -> DeployServiceResult<CertificateOrderResponse>;

    async fn fail_certificate_order(
        &self,
        context: &DeployBackendRequestContext,
        order_id: &str,
        error_code: &str,
    ) -> DeployServiceResult<()>;

    async fn record_challenge_result(
        &self,
        context: &DeployBackendRequestContext,
        order_id: &str,
        challenge_id: Option<&str>,
        valid: bool,
    ) -> DeployServiceResult<()>;

    async fn store_certificate_version(
        &self,
        context: &DeployBackendRequestContext,
        request: &StoreCertificateVersionRequest,
    ) -> DeployServiceResult<CertificateOrderResponse>;

    async fn list_certificate_orders(
        &self,
        context: &DeployBackendRequestContext,
        certificate_id: &str,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<CertificateOrderPage>;

    async fn list_certificate_challenges(
        &self,
        context: &DeployBackendRequestContext,
        order_id: &str,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<CertificateChallengePage>;

    async fn run_retention(
        &self,
        context: &DeployBackendRequestContext,
        request: &RetentionRunRequest,
    ) -> DeployServiceResult<RetentionRunResponse>;

    async fn ingest_usage_events(
        &self,
        context: &DeployBackendRequestContext,
        request: &IngestUsageEventsRequest,
    ) -> DeployServiceResult<UsageIngestResult>;

    async fn rebuild_usage_daily(
        &self,
        context: &DeployBackendRequestContext,
        request: &UsageReconciliationRequest,
    ) -> DeployServiceResult<UsageReconciliationResponse>;

    async fn list_signing_identity_health(
        &self,
        context: &DeployBackendRequestContext,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<SigningIdentityHealthPage>;

    /// Reconciles the factory's soft cross-module references (section 9): the
    /// identity anchor, the hosting carrier, the installation domains, and the
    /// token anchor. Read-only; never repairs. A dangling reference is an alert,
    /// not a valid state, so this answers `DRIFT` rather than silently passing.
    async fn factory_reconciliation(
        &self,
        context: &DeployBackendRequestContext,
    ) -> DeployServiceResult<AppFactoryReconciliation>;

    /// Ingests a Git webhook push event (GitHub-compatible payload with
    /// `X-Hub-Signature-256` HMAC verification) and triggers builds for the
    /// matched repository's active targets on the default branch.
    async fn ingest_source_event(
        &self,
        context: &DeployBackendRequestContext,
        payload: &[u8],
        signature: Option<&str>,
    ) -> DeployServiceResult<SourceEventIngestResponse>;

    async fn list_source_events(
        &self,
        context: &DeployBackendRequestContext,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<SourceEventPage>;

    // -- app scaffold marketplace (admin) --------------------------------------

    async fn list_scaffold_categories(
        &self,
        context: &DeployBackendRequestContext,
        include_disabled: bool,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<crate::scaffold_market::ScaffoldCategoryPage>;

    async fn create_scaffold_category(
        &self,
        context: &DeployBackendRequestContext,
        request: &crate::scaffold_market::CreateScaffoldCategoryRequest,
    ) -> DeployServiceResult<crate::scaffold_market::ScaffoldCategoryResponse>;

    async fn retrieve_scaffold_category(
        &self,
        context: &DeployBackendRequestContext,
        category_uuid: &str,
    ) -> DeployServiceResult<crate::scaffold_market::ScaffoldCategoryResponse>;

    async fn update_scaffold_category(
        &self,
        context: &DeployBackendRequestContext,
        category_uuid: &str,
        request: &crate::scaffold_market::UpdateScaffoldCategoryRequest,
    ) -> DeployServiceResult<crate::scaffold_market::ScaffoldCategoryResponse>;

    async fn delete_scaffold_category(
        &self,
        context: &DeployBackendRequestContext,
        category_uuid: &str,
    ) -> DeployServiceResult<()>;

    async fn list_application_scaffolds(
        &self,
        context: &DeployBackendRequestContext,
        query: &crate::scaffold_market::ListApplicationScaffoldsAdminQuery,
    ) -> DeployServiceResult<crate::scaffold_market::ApplicationScaffoldPage>;

    async fn retrieve_application_scaffold(
        &self,
        context: &DeployBackendRequestContext,
        scaffold_uuid: &str,
    ) -> DeployServiceResult<crate::scaffold_market::ApplicationScaffoldResponse>;

    async fn update_application_scaffold(
        &self,
        context: &DeployBackendRequestContext,
        scaffold_uuid: &str,
        request: &crate::scaffold_market::UpdateApplicationScaffoldAdminRequest,
    ) -> DeployServiceResult<crate::scaffold_market::ApplicationScaffoldResponse>;

    async fn delete_application_scaffold(
        &self,
        context: &DeployBackendRequestContext,
        scaffold_uuid: &str,
    ) -> DeployServiceResult<()>;

    async fn list_application_scaffold_versions(
        &self,
        context: &DeployBackendRequestContext,
        scaffold_uuid: &str,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<crate::scaffold_market::ApplicationScaffoldVersionPage>;

    async fn retrieve_application_scaffold_version(
        &self,
        context: &DeployBackendRequestContext,
        scaffold_uuid: &str,
        version_uuid: &str,
    ) -> DeployServiceResult<crate::scaffold_market::ApplicationScaffoldVersionResponse>;

    // -- application identity (operator retry entry) ----------------------------

    /// Re-runs the IAM installation provisioning for one application.
    ///
    /// `apps.create` is not the only entry any more: an application whose create
    /// ran while IAM was unreachable stays `pending_identity`/`failed` with no
    /// way back, because a replay of `apps.create` is only possible while the
    /// original `Idempotency-Key` and request body are still known. This command
    /// is that way back. It reads the current status first (`active` is returned
    /// unchanged and calls nothing), then replays the stored
    /// `deploy_app.identity_provisioning_key` (`deploy:{id}`) so IAM lands on the
    /// installation the original attempt would have created, and records the
    /// resulting status on the row.
    async fn retry_app_identity_provisioning(
        &self,
        context: &DeployBackendRequestContext,
        app_id: &str,
    ) -> DeployServiceResult<crate::app_delivery::AppIdentityProvisionResponse>;
}
