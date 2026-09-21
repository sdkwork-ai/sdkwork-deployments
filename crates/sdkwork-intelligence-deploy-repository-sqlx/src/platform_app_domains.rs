//! Platform app publishing domains: idempotent provisioning of every app's
//! default publishable hostnames (`<appDomainLabel>.app[-<env>].<suffix>`) and
//! the hostname → app resolution the Web Server fallback uses.
//!
//! The platform owns the apex domains (`app.<suffix>`), so provisioned
//! hostnames are automatically `VERIFIED`; custom domains keep the regular
//! DNS verification flow (`domain_zones.rs`).
//!
//! Two configuration facts live on `deploy_app` and drive everything here:
//!
//! - `app_domain_label` — the `<appId>` prefix of the default hostnames. It
//!   defaults to the slug and may be replaced by any DNS label (including the
//!   app's own uuid), which is the "custom prefix replaces the app id"
//!   capability. `slug` remains the fallback so the historical catalog keeps
//!   working unchanged.
//! - `app_domain_suffixes` — an optional per-app suffix override. `NULL`
//!   means the platform catalog (`PLATFORM_APP_DOMAIN_SUFFIXES`).
//!
//! The nginx configuration is `deploy_app.nginx_conf` (app-level base) with an
//! optional environment- or hostname-scoped override in `deploy_nginx_config`
//! (`environment`/`hostname_ascii`). The resolution below applies the
//! precedence hostname+environment → environment → app base.

use sdkwork_deploy_contract::{
    AppDomainPage, AppDomainResponse, DeployServiceError, DeployServiceResult,
    ProvisionAppDomainsResult, ResolvedDeployServer,
};
use sdkwork_deploy_core::{app_domain_label, default_app_hostname};
use sqlx::Row;

use crate::support::{new_uuid, next_id, now_rfc3339, resolve_app_internal_id, store_error};
use crate::DeployRepository;

/// Batch traffic usage ingest for the Web Server usage metering
/// (inherent method so read-only consumers share one entry point).
impl DeployRepository {
    pub async fn ingest_usage_events_lookup(
        &self,
        events: &[sdkwork_deploy_contract::UsageEventIngestItem],
    ) -> sdkwork_deploy_contract::DeployServiceResult<sdkwork_deploy_contract::UsageIngestResult>
    {
        self.insert_usage_events_batch_repo(events).await
    }
}

/// Read-only hostname resolution for the Web Server app-domain fallback.
/// Inherent method so read-only consumers (for example the standalone
/// gateway, which shares the Deploy database) resolve servers without the
/// service port trait.
impl DeployRepository {
    pub async fn resolve_server_by_hostname_lookup(
        &self,
        hostname: &str,
        environment: &str,
    ) -> DeployServiceResult<Option<ResolvedDeployServer>> {
        self.resolve_active_app_by_hostname_repo(hostname, environment)
            .await
    }
}

/// Zone apex for one platform suffix: `app.<suffix>`.
fn platform_zone_apex(suffix: &str) -> String {
    format!("app.{suffix}")
}

/// Ensure the platform zone (`app.<suffix>`) and its auto-verified apex domain
/// exist for the tenant. Returns the zone id and whether it was created.
///
/// Self-healing on purpose: provisioning an app whose `appDomainSuffixes`
/// names a suffix outside the platform catalog must create the zone it needs
/// rather than fail with a not-found error, so this is shared by the explicit
/// tenant pre-provisioning entry point and by the per-app reconcile path.
///
/// Because it *creates* a zone, it has to answer the two questions the
/// operator-facing `create_domain_zone_repo` answers before it writes, or it
/// corrupts the inventory instead of refusing:
///
/// 1. the apex is already taken — possibly by another tenant, because
///    `uk_deploy_dns_zone_active_apex` is a *global* unique index; and
/// 2. the apex overlaps an existing zone, which happens as soon as an operator
///    has defined the bare `example.com` that the platform now wants
///    `app.example.com` under. Nesting zones silently makes DNS-01 challenge
///    resolution ambiguous and mints `VERIFIED` hostnames inside a domain the
///    operator owns.
async fn ensure_platform_zone_in_tx(
    id_generator: &sdkwork_database_id::SnowflakeIdGenerator,
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    tenant_id: i64,
    organization_id: i64,
    actor_id: Option<i64>,
    suffix: &str,
) -> DeployServiceResult<(i64, bool)> {
    let apex = platform_zone_apex(suffix);
    // Looked up by apex alone, across tenants, for the same reason the unique
    // index is global: a tenant-scoped lookup answers "free" for an apex another
    // tenant holds, and the INSERT that follows dies as a raw constraint
    // violation (a masked 500) instead of a diagnosable conflict.
    let existing: Option<(i64, i64, Option<i64>)> = sqlx::query_as(
        "SELECT id, tenant_id, user_id FROM deploy_dns_zone
         WHERE apex_hostname = $1 AND deleted_at IS NULL",
    )
    .bind(&apex)
    .fetch_optional(&mut **transaction)
    .await
    .map_err(|error| store_error("lookup platform app zone", error))?;
    if let Some((zone_id, owner_tenant_id, owner_user_id)) = existing {
        // Only a platform-scoped zone of this tenant is reused. Anything else —
        // another tenant's zone, or an operator's own root domain — is a real
        // conflict: reusing it would bind this app's publishing domains into a
        // namespace this tenant does not own.
        if owner_tenant_id == tenant_id && owner_user_id.is_none() {
            return Ok((zone_id, false));
        }
        return Err(DeployServiceError::conflict(format!(
            "platform publishing zone {apex} is already registered by another tenant or by a user-defined root domain"
        )));
    }
    // No zone owns the apex, but an existing zone may still contain it or be
    // contained by it. Refusing here preserves the "zones never overlap"
    // invariant that `create_domain_zone_repo` enforces for operator zones.
    let overlapping: Option<String> = sqlx::query_scalar(
        "SELECT apex_hostname FROM deploy_dns_zone
         WHERE deleted_at IS NULL
           AND (apex_hostname LIKE '%.' || $1 OR $1 LIKE '%.' || apex_hostname)
         LIMIT 1",
    )
    .bind(&apex)
    .fetch_optional(&mut **transaction)
    .await
    .map_err(|error| store_error("check platform app zone overlap", error))?;
    if let Some(existing_apex) = overlapping {
        return Err(DeployServiceError::conflict(format!(
            "platform publishing zone {apex} overlaps the existing domain zone {existing_apex}"
        )));
    }
    let zone_id = next_id(id_generator)?;
    // `user_id` stays NULL on purpose: a platform zone is tenant-level
    // infrastructure rather than any one user's domain, which is exactly what
    // keeps it visible to every member of the tenant (see `zone_owner_gate`).
    sqlx::query(
        "INSERT INTO deploy_dns_zone (
            id, uuid, tenant_id, organization_id, apex_hostname, display_name,
            dns_provider, provider_zone_ref, status, user_id, created_by, updated_by
         ) VALUES ($1, $2, $3, $4, $5, $6, 'platform', $7, 'ACTIVE', NULL, $8, $8)",
    )
    .bind(zone_id)
    .bind(new_uuid())
    .bind(tenant_id)
    .bind(organization_id)
    .bind(&apex)
    .bind(format!("Platform app domain zone {suffix}"))
    .bind(format!("app.*.{suffix}"))
    .bind(actor_id)
    .execute(&mut **transaction)
    .await
    .map_err(|error| store_error("insert platform app zone", error))?;
    // The zone apex is platform-owned and therefore auto-verified.
    let now = now_rfc3339();
    sqlx::query(
        "INSERT INTO deploy_domain (
            id, uuid, tenant_id, organization_id, zone_id, hostname_ascii, hostname_type,
            verification_status, verified_at, status, created_by, updated_by
         ) VALUES ($1, $2, $3, $4, $5, $6, 'EXACT', 'VERIFIED', CAST($7 AS TIMESTAMPTZ),
            'ACTIVE', $8, $8)",
    )
    .bind(next_id(id_generator)?)
    .bind(new_uuid())
    .bind(tenant_id)
    .bind(organization_id)
    .bind(zone_id)
    .bind(&apex)
    .bind(&now)
    .bind(actor_id)
    .execute(&mut **transaction)
    .await
    .map_err(|error| store_error("insert platform app zone apex domain", error))?;
    Ok((zone_id, true))
}

/// The binding keyspace reserved for auto-provisioned default publishing
/// domains. Reconciliation only ever retires rows in this namespace, so a
/// composition-declared binding on a platform hostname is never touched.
pub(crate) const DEFAULT_BINDING_KEY_PREFIX: &str = "appd-";

impl DeployRepository {
    /// The app's effective publishing configuration: the `<appId>` prefix and
    /// the suffix catalog. Read in one round trip because both provisioning
    /// and the Web Server lookup depend on them.
    pub(super) async fn app_domain_config_repo(
        &self,
        app_id: i64,
    ) -> DeployServiceResult<AppDomainConfig> {
        let row = sqlx::query(
            "SELECT slug, app_domain_label, app_domain_suffixes
             FROM deploy_app WHERE id = $1 AND deleted_at IS NULL",
        )
        .bind(app_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("read app domain config", error))?;
        let Some(row) = row else {
            return Err(DeployServiceError::not_found("app not found"));
        };
        let slug: String = row
            .try_get("slug")
            .map_err(|error| DeployServiceError::Internal(format!("read app slug: {error}")))?;
        let app_domain_label: Option<String> = row.try_get("app_domain_label").ok().flatten();
        // Decoded through the JSONB helper (not `try_get::<Option<Vec<String>>>`,
        // which sqlx maps onto `TEXT[]` and which therefore fails against this
        // `JSONB` column). An empty array means "no override", matching the
        // writer, which never stores one.
        let override_suffixes = crate::support::string_list_from_row(&row, "app_domain_suffixes")
            .map_err(|error| DeployServiceError::Internal(format!(
                "read app domain suffixes: {error}"
            )))?
            .filter(|suffixes| !suffixes.is_empty());
        Ok(AppDomainConfig {
            label: sdkwork_deploy_core::effective_app_domain_label(
                app_domain_label.as_deref(),
                &slug,
            )
            .to_owned(),
            slug,
            suffixes: sdkwork_deploy_core::effective_app_domain_suffixes(
                override_suffixes.as_deref(),
            ),
            override_suffixes,
        })
    }

    /// Every hostname the app answers on, ordered default-first.
    ///
    /// Reads `deploy_app_binding` — the same rows the Web Server hostname
    /// lookup matches on — so the console's domain column cannot disagree with
    /// what is actually routable. A hostname whose binding key starts with
    /// [`DEFAULT_BINDING_KEY_PREFIX`] is an auto-provisioned platform domain
    /// (hence `DEFAULT` and already `VERIFIED`, because the platform owns the
    /// zone); anything else is a user-owned custom hostname and carries the
    /// `CNAME` target the user has to create.
    pub(super) async fn list_app_domains_repo(
        &self,
        tenant_id: i64,
        app_id: &str,
    ) -> DeployServiceResult<AppDomainPage> {
        let internal_id = resolve_app_internal_id(&self.pool, tenant_id, app_id).await?;
        // The CNAME target for custom hostnames is the app's own canonical
        // default hostname: aliasing to a hostname rather than to an IP keeps
        // the certificate (issued for the platform wildcard) valid and lets the
        // platform move servers without asking users to re-point DNS.
        let config = self.app_domain_config_repo(internal_id).await?;
        let suffix = config
            .suffixes
            .first()
            .cloned()
            .unwrap_or_else(|| "sdkwork.com".to_owned());
        let rows = sqlx::query(
            "SELECT b.hostname_ascii, b.binding_key, b.environment, b.path_prefix,
                    b.action_type, b.status, b.is_canonical,
                    b.verified_at IS NOT NULL AS binding_verified,
                    d.uuid AS domain_uuid, d.hostname_type,
                    d.verification_status AS domain_verification_status,
                    z.apex_hostname AS zone_apex
             FROM deploy_app_binding b
             LEFT JOIN deploy_domain d ON d.id = b.domain_id AND d.deleted_at IS NULL
             LEFT JOIN deploy_dns_zone z ON z.id = d.zone_id AND z.deleted_at IS NULL
             WHERE b.app_id = $1 AND b.deleted_at IS NULL
             ORDER BY (b.binding_key NOT LIKE $2) ASC,
                      b.environment, b.hostname_ascii, b.path_prefix",
        )
        .bind(internal_id)
        .bind(format!("{DEFAULT_BINDING_KEY_PREFIX}%"))
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error("list app binding rows", error))?;

        let mut items = Vec::with_capacity(rows.len());
        for row in rows {
            let hostname: String = row
                .try_get("hostname_ascii")
                .map_err(|error| DeployServiceError::Internal(format!("read hostname: {error}")))?;
            let binding_key: Option<String> = row.try_get("binding_key").ok().flatten();
            let is_default = binding_key
                .as_deref()
                .map(|key| key.starts_with(DEFAULT_BINDING_KEY_PREFIX))
                .unwrap_or(false);
            let environment: String = row.try_get("environment").unwrap_or_default();
            let binding_status: String = row.try_get("status").unwrap_or_default();
            let domain_verification: Option<String> =
                row.try_get("domain_verification_status").ok().flatten();
            let binding_verified: bool = row.try_get("binding_verified").unwrap_or(false);
            // A default hostname lives in a platform-owned zone, so verification
            // is *not required* — reporting it as PENDING would show the user a
            // DNS step that does not exist. A custom hostname falls back to the
            // binding's own verification when the domain row was not joined.
            let verification_status = if is_default {
                "NOT_REQUIRED".to_owned()
            } else {
                match domain_verification.as_deref() {
                    Some("VERIFIED") => "VERIFIED".to_owned(),
                    Some("FAILED") => "FAILED".to_owned(),
                    Some("EXPIRED") => "EXPIRED".to_owned(),
                    Some(_) => "PENDING".to_owned(),
                    None if binding_verified => "VERIFIED".to_owned(),
                    None => "PENDING".to_owned(),
                }
            };
            let zone_apex: Option<String> = row.try_get("zone_apex").ok().flatten();
            let hostname_type: Option<String> = row.try_get("hostname_type").ok().flatten();
            // Only a custom label under a zone the *user* owns needs a DNS
            // record; `WILDCARD` rows are platform-managed. The record name is
            // the label with the zone apex stripped.
            let dns_record_name = if !is_default && hostname_type.as_deref() != Some("WILDCARD") {
                zone_apex
                    .as_deref()
                    .and_then(|apex| {
                        hostname
                            .strip_suffix(apex)
                            .map(|label| label.trim_end_matches('.'))
                    })
                    .map(str::to_owned)
                    .filter(|label| !label.is_empty())
                    .or_else(|| Some(hostname.clone()))
            } else {
                None
            };
            items.push(AppDomainResponse {
                hostname: hostname.clone(),
                kind: if is_default { "DEFAULT" } else { "CUSTOM" }.to_owned(),
                environment: environment.clone(),
                binding_status,
                verification_status: verification_status.clone(),
                is_canonical: row.try_get("is_canonical").ok(),
                path_prefix: row.try_get("path_prefix").ok().flatten(),
                domain_id: row.try_get("domain_uuid").ok().flatten(),
                // Only a custom hostname still awaiting verification needs the
                // instruction; a verified one must not keep showing a stale CNAME.
                dns_record_value: if verification_status == "PENDING" {
                    Some(hostname.clone())
                } else {
                    None
                },
                dns_record_name,
                cname_target: Some(default_app_hostname(&config.label, &suffix, "production")),
            });
        }
        let total = items.len() as i64;
        Ok(AppDomainPage { items, total })
    }

    /// Create the platform app-domain DNS zones for a tenant for the supplied
    /// suffix catalog (apex `app.<suffix>`) and their apex hostname rows.
    /// Idempotent: existing zones are kept and not counted. Returns the number
    /// of newly created zones.
    pub(super) async fn ensure_platform_app_zones_repo(
        &self,
        tenant_id: i64,
        organization_id: i64,
        actor_id: Option<i64>,
        suffixes: &[String],
    ) -> DeployServiceResult<usize> {
        let mut created = 0;
        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error("begin platform app zones", error))?;
        for suffix in suffixes {
            let (_, zone_created) = ensure_platform_zone_in_tx(
                self.id_generator(),
                &mut transaction,
                tenant_id,
                organization_id,
                actor_id,
                suffix,
            )
            .await?;
            if zone_created {
                created += 1;
            }
        }
        transaction
            .commit()
            .await
            .map_err(|error| store_error("commit platform app zones", error))?;
        Ok(created)
    }

    /// Idempotently provision an app's default publishing domains for one
    /// lifecycle environment: for every effective suffix, an EXACT
    /// `deploy_domain` (`<appDomainLabel>.app[-<env>].<suffix>`,
    /// auto-verified) and a `SERVE` binding on the app. The first suffix
    /// binding is the canonical one.
    ///
    /// The operation is a **reconcile**: rows in the auto-provisioned
    /// keyspace (`appd-*`) whose hostname is no longer part of the effective
    /// catalog are retired (soft-deleted) in the same transaction, so renaming
    /// an app or changing its `appDomainLabel`/`appDomainSuffixes` never leaves
    /// a stale publishable hostname behind. User-declared bindings are never
    /// touched.
    pub(super) async fn provision_app_default_domains_repo(
        &self,
        tenant_id: i64,
        organization_id: i64,
        actor_id: Option<i64>,
        app_id: &str,
        environment: &str,
    ) -> DeployServiceResult<ProvisionAppDomainsResult> {
        let app_id = crate::support::resolve_app_internal_id(&self.pool, tenant_id, app_id).await?;
        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error("begin provision app default domains", error))?;
        let result = reconcile_app_default_domains_tx(
            self,
            &mut transaction,
            tenant_id,
            organization_id,
            actor_id,
            app_id,
            environment,
        )
        .await?;
        transaction
            .commit()
            .await
            .map_err(|error| store_error("commit provision app default domains", error))?;
        Ok(result)
    }
}

/// In-transaction reconcile of an app's default publishing domains for one
/// environment. Shared by the standalone provisioning entry point and by the
/// composition replace path, which must create the platform publishing domains
/// inside the same transaction that rewrites the app's bindings.
pub(crate) async fn reconcile_app_default_domains_tx(
    repository: &DeployRepository,
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    tenant_id: i64,
    organization_id: i64,
    actor_id: Option<i64>,
    app_internal_id: i64,
    environment: &str,
) -> DeployServiceResult<ProvisionAppDomainsResult> {
    let config = repository.app_domain_config_repo(app_internal_id).await?;
    let app_id = app_internal_id;
    let mut result = ProvisionAppDomainsResult::default();
    let label = app_domain_label(environment);
    let expected_hostnames = config
        .suffixes
        .iter()
        .map(|suffix| default_app_hostname(&config.label, suffix, environment))
        .collect::<Vec<_>>();
    // Retire the environment's outdated auto-provisioned bindings *before*
    // inserting the current ones. A changed `appDomainLabel` reuses the same
    // `appd-<environmentLabel>-<index>` binding keys, so the stale rows must
    // release the key first (the unique index only covers live rows).
    retire_stale_default_bindings_tx(transaction, app_id, environment, &expected_hostnames).await?;
    for (index, suffix) in config.suffixes.iter().enumerate() {
        let hostname = expected_hostnames[index].clone();
        let (zone_id, zone_created) = ensure_platform_zone_in_tx(
            repository.id_generator(),
            transaction,
            tenant_id,
            organization_id,
            actor_id,
            suffix,
        )
        .await?;
        if zone_created {
            result.created_zones += 1;
        }
        let domain_id: Option<i64> = sqlx::query_scalar(
            "SELECT id FROM deploy_domain
             WHERE tenant_id = $1 AND hostname_ascii = $2 AND deleted_at IS NULL",
        )
        .bind(tenant_id)
        .bind(&hostname)
        .fetch_optional(&mut **transaction)
        .await
        .map_err(|error| store_error("lookup app default domain", error))?;
        // The lookup above keys on `hostname_ascii` alone, because that is the
        // column the global unique index covers. The row it finds must sit in
        // *this* platform zone: reusing one that lives under an operator's own
        // zone would bind the app to a hostname the operator controls while the
        // platform reports the domain as provisioned, and would silently make
        // the operator's zone undeletable.
        if let Some(existing_id) = domain_id {
            let existing_zone_id: i64 =
                sqlx::query_scalar("SELECT zone_id FROM deploy_domain WHERE id = $1")
                    .bind(existing_id)
                    .fetch_one(&mut **transaction)
                    .await
                    .map_err(|error| store_error("read app default domain zone", error))?;
            if existing_zone_id != zone_id {
                return Err(DeployServiceError::conflict(format!(
                    "default publishing hostname {hostname} is already registered under a different domain zone"
                )));
            }
        }
        // `hostname_ascii` is unique across all active domains, so a
        // default publishing hostname claimed by another tenant must
        // fail with a clear conflict instead of a generic constraint
        // violation (the app-domain label must be unique platform-wide for
        // the default `<label>.app[-<env>].<suffix>` catalog).
        let cross_tenant: Option<i64> = sqlx::query_scalar(
            "SELECT id FROM deploy_domain
             WHERE hostname_ascii = $1 AND deleted_at IS NULL AND tenant_id <> $2
             LIMIT 1",
        )
        .bind(&hostname)
        .bind(tenant_id)
        .fetch_optional(&mut **transaction)
        .await
        .map_err(|error| store_error("lookup cross-tenant app default domain", error))?;
        if domain_id.is_none() && cross_tenant.is_some() {
            return Err(DeployServiceError::conflict(format!(
                "default publishing hostname {hostname} is already registered by another tenant; choose a different app slug or appDomainLabel"
            )));
        }
        let domain_id = match domain_id {
            Some(existing) => {
                result.existing_domains += 1;
                existing
            }
            None => {
                let id = next_id(repository.id_generator())?;
                let now = now_rfc3339();
                sqlx::query(
                    "INSERT INTO deploy_domain (
                        id, uuid, tenant_id, organization_id, zone_id, hostname_ascii,
                        hostname_type, verification_status, verified_at, status,
                        created_by, updated_by
                     ) VALUES ($1, $2, $3, $4, $5, $6, 'EXACT', 'VERIFIED',
                        CAST($7 AS TIMESTAMPTZ), 'ACTIVE', $8, $8)",
                )
                .bind(id)
                .bind(new_uuid())
                .bind(tenant_id)
                .bind(organization_id)
                .bind(zone_id)
                .bind(&hostname)
                .bind(&now)
                .bind(actor_id)
                .execute(&mut **transaction)
                .await
                .map_err(|error| store_error("insert app default domain", error))?;
                result.created_domains += 1;
                id
            }
        };
        let binding_exists: Option<i64> = sqlx::query_scalar(
            "SELECT id FROM deploy_app_binding
             WHERE app_id = $1 AND hostname_ascii = $2 AND environment = $3
               AND deleted_at IS NULL",
        )
        .bind(app_id)
        .bind(&hostname)
        .bind(environment)
        .fetch_optional(&mut **transaction)
        .await
        .map_err(|error| store_error("lookup app default binding", error))?;
        if binding_exists.is_some() {
            result.existing_bindings += 1;
            result.hostnames.push(hostname);
            continue;
        }
        let binding_id = next_id(repository.id_generator())?;
        let now = now_rfc3339();
        let binding_key = format!("{DEFAULT_BINDING_KEY_PREFIX}{label}-{index}");
        sqlx::query(
            "INSERT INTO deploy_app_binding (
                id, uuid, tenant_id, organization_id, app_id, binding_key, domain_id,
                hostname_ascii, environment, path_prefix, action_type, is_canonical,
                status, verified_at, activated_at, created_by, updated_by,
                created_at, updated_at, version
             ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, '/', 'SERVE', $10,
                'ACTIVE', CAST($11 AS TIMESTAMPTZ), CAST($11 AS TIMESTAMPTZ), $12, $12,
                CAST($11 AS TIMESTAMPTZ), CAST($11 AS TIMESTAMPTZ), 1)",
        )
        .bind(binding_id)
        .bind(new_uuid())
        .bind(tenant_id)
        .bind(organization_id)
        .bind(app_id)
        .bind(&binding_key)
        .bind(domain_id)
        .bind(&hostname)
        .bind(environment)
        .bind(index == 0)
        .bind(&now)
        .bind(actor_id)
        .execute(&mut **transaction)
        .await
        .map_err(|error| store_error("insert app default binding", error))?;
        result.created_bindings += 1;
        result.hostnames.push(hostname);
    }
    Ok(result)
}

/// Soft-delete this environment's auto-provisioned publishing bindings whose
/// hostname is no longer part of the effective catalog. Only the `appd-*`
/// keyspace is ever touched, so a composition-declared binding on a platform
/// hostname survives.
async fn retire_stale_default_bindings_tx(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    app_id: i64,
    environment: &str,
    expected_hostnames: &[String],
) -> DeployServiceResult<u64> {
    let retired = sqlx::query(
        "UPDATE deploy_app_binding
         SET deleted_at = NOW(), status = 'ARCHIVED', updated_at = NOW(), version = version + 1
         WHERE app_id = $1 AND environment = $2
           AND binding_key LIKE $3
           AND deleted_at IS NULL
           AND NOT (hostname_ascii = ANY($4))",
    )
    .bind(app_id)
    .bind(environment)
    .bind(format!("{DEFAULT_BINDING_KEY_PREFIX}%"))
    .bind(expected_hostnames)
    .execute(&mut **transaction)
    .await
    .map_err(|error| store_error("retire stale app default bindings", error))?
    .rows_affected();
    if retired > 0 {
        tracing::info!(
            app_id,
            environment,
            retired,
            "retired stale auto-provisioned app publishing bindings"
        );
    }
    Ok(retired)
}

impl DeployRepository {
    /// Resolve an active app binding by its exact hostname in one lifecycle
    /// environment and return the *environment-scoped* compiled website
    /// runtime descriptor (`deploy_app_revision.descriptor_json`) together
    /// with the app's nginx configuration. This is the Web Server fallback
    /// lookup: custom domains and default app domains both land here (default
    /// app bindings are explicit rows).
    ///
    /// Version selection is per `(app, environment)`: the newest `VALID`
    /// revision **of that environment**. The app-level
    /// `deploy_app.current_revision_id` pointer is deliberately not used —
    /// it is written by whichever environment last converged its runtime
    /// assignments, so it would serve one environment's composition on
    /// another environment's hostname.
    pub(super) async fn resolve_active_app_by_hostname_repo(
        &self,
        hostname: &str,
        environment: &str,
    ) -> DeployServiceResult<Option<ResolvedDeployServer>> {
        let hostname = hostname.trim().to_ascii_lowercase();
        if hostname.is_empty() || hostname.len() > 253 || hostname.ends_with('.') {
            return Err(DeployServiceError::validation(
                "hostname must be normalized lowercase ASCII without a trailing dot",
            ));
        }
        let row = sqlx::query(
            "SELECT s.uuid AS app_uuid,
                    s.slug AS app_slug,
                    COALESCE(s.app_domain_label, s.slug) AS app_domain_label,
                    b.tenant_id, b.hostname_ascii, b.path_prefix, b.action_type,
                    b.uuid AS binding_uuid, b.environment,
                    r.descriptor_json, r.descriptor_sha256, r.revision_no,
                    COALESCE(n.config_content, s.nginx_conf) AS nginx_conf,
                    COALESCE(n.config_hash, s.nginx_conf_sha256) AS nginx_conf_sha256
             FROM deploy_app_binding b
             JOIN deploy_app s
               ON s.id = b.app_id
              AND s.deleted_at IS NULL
              AND s.app_status = 'ACTIVE'
             JOIN LATERAL (
                 SELECT revision.descriptor_json, revision.descriptor_sha256,
                        revision.revision_no
                 FROM deploy_app_revision revision
                 WHERE revision.app_id = b.app_id
                   AND revision.environment = b.environment
                   AND revision.validation_status = 'VALID'
                 ORDER BY revision.revision_no DESC
                 LIMIT 1
             ) r ON TRUE
             LEFT JOIN LATERAL (
                 SELECT config.config_content, config.config_hash
                 FROM deploy_nginx_config config
                 WHERE config.app_id = b.app_id
                   AND config.is_active = TRUE
                   AND config.status = 1
                   AND (config.environment IS NULL OR config.environment = b.environment)
                   AND (config.hostname_ascii IS NULL
                        OR config.hostname_ascii = b.hostname_ascii)
                 ORDER BY (config.hostname_ascii IS NOT NULL) DESC,
                          (config.environment IS NOT NULL) DESC,
                          config.version_no DESC, config.id DESC
                 LIMIT 1
             ) n ON TRUE
             WHERE b.hostname_ascii = $1 AND b.environment = $2
               AND b.status = 'ACTIVE' AND b.deleted_at IS NULL
             ORDER BY (b.path_prefix = '/') DESC, b.path_prefix DESC, b.id DESC
             LIMIT 1",
        )
        .bind(&hostname)
        .bind(environment)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("resolve active app by hostname", error))?;
        let Some(row) = row else {
            return Ok(None);
        };
        let app_uuid: String = row
            .try_get("app_uuid")
            .map_err(|error| DeployServiceError::Internal(format!("read app uuid: {error}")))?;
        let app_slug: String = row
            .try_get("app_slug")
            .map_err(|error| DeployServiceError::Internal(format!("read app slug: {error}")))?;
        let app_domain_label: String = row.try_get("app_domain_label").map_err(|error| {
            DeployServiceError::Internal(format!("read app domain label: {error}"))
        })?;
        let tenant_id: i64 = row
            .try_get("tenant_id")
            .map_err(|error| DeployServiceError::Internal(format!("read tenant id: {error}")))?;
        let binding_uuid: Option<String> = row.try_get("binding_uuid").ok();
        let hostname: String = row
            .try_get("hostname_ascii")
            .map_err(|error| DeployServiceError::Internal(format!("read hostname: {error}")))?;
        let path_prefix: String = row
            .try_get("path_prefix")
            .map_err(|error| DeployServiceError::Internal(format!("read path prefix: {error}")))?;
        let action_type: String = row
            .try_get("action_type")
            .map_err(|error| DeployServiceError::Internal(format!("read action type: {error}")))?;
        let descriptor_json: serde_json::Value = row
            .try_get("descriptor_json")
            .map_err(|error| DeployServiceError::Internal(format!("read descriptor: {error}")))?;
        let descriptor_sha256: String = row.try_get("descriptor_sha256").map_err(|error| {
            DeployServiceError::Internal(format!("read descriptor hash: {error}"))
        })?;
        let revision_no: i64 = row
            .try_get("revision_no")
            .map_err(|error| DeployServiceError::Internal(format!("read revision no: {error}")))?;
        let environment: String = row
            .try_get("environment")
            .map_err(|error| DeployServiceError::Internal(format!("read environment: {error}")))?;
        let nginx_conf: Option<String> = row.try_get("nginx_conf").ok().flatten();
        let nginx_conf_sha256: Option<String> = row.try_get("nginx_conf_sha256").ok().flatten();
        Ok(Some(ResolvedDeployServer {
            app_uuid: app_uuid.clone(),
            app_slug,
            hostname,
            path_prefix,
            action_type,
            tenant_id,
            app_id: Some(app_uuid),
            binding_id: binding_uuid,
            descriptor_json,
            descriptor_sha256,
            revision_no,
            environment,
            nginx_conf,
            nginx_conf_sha256,
            app_domain_label,
        }))
    }
}

/// Effective publishing configuration of one app.
#[derive(Clone, Debug)]
pub struct AppDomainConfig {
    /// The `<appId>` prefix used in default hostnames.
    pub label: String,
    pub slug: String,
    /// The catalog actually used (`override_suffixes` or the platform
    /// catalog).
    pub suffixes: Vec<String>,
    /// The raw `appDomainSuffixes` override, when the app declares one.
    pub override_suffixes: Option<Vec<String>>,
}

fn platform_suffixes() -> Vec<String> {
    sdkwork_deploy_core::PLATFORM_APP_DOMAIN_SUFFIXES
        .iter()
        .map(|suffix| (*suffix).to_owned())
        .collect()
}
