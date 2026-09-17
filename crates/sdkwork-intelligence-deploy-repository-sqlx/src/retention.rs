//! Retention enforcement, usage daily reconciliation, and signing identity
//! health (PRD §5.8, TECH §8): bounded housekeeping over packages, releases,
//! and build logs, the rebuildable daily usage aggregate, and the signing
//! expiry management surface.

use sdkwork_deploy_contract::{
    DeployServiceError, DeployServiceResult, RetentionRunResponse, SigningIdentityHealthPage,
    SigningIdentityHealthResponse, UsageReconciliationResponse,
};
use sqlx::{AssertSqlSafe, Row};

use crate::support::{next_id, optional_datetime, pagination, store_error};
use crate::DeployRepository;

/// Which daily rollup a usage rebuild writes.
#[derive(Clone, Copy)]
enum UsageDailyScope {
    /// Per app (and binding) rollup in `deploy_app_usage_daily`. Only facts
    /// that carry an app attribution can enter it — the table's `app_id` is
    /// NOT NULL with a foreign key to `deploy_app`.
    App,
    /// Tenant-wide rollup in `deploy_tenant_usage_daily`. This is the superset:
    /// every fact in the window, including unmanaged traffic that no app
    /// attributes (recorded against tenant 0, which is how the producers tag
    /// it).
    Tenant,
}

/// One reduced daily aggregate row, already summed so it can be written back
/// with an id minted by the application.
struct UsageDailyRow {
    tenant_id: i64,
    organization_id: i64,
    app_id: Option<i64>,
    binding_id: Option<i64>,
    usage_date: chrono::NaiveDate,
    dimension: String,
    quantity: i64,
    unit: String,
    source_revision: String,
}

impl DeployRepository {
    /// Applies retention policies. `dry_run` reports the candidate counts
    /// without mutating; real runs retire unreferenced packages/releases past
    /// their retention windows and purge expired build log references.
    /// Retention windows come from the caller (platform configuration); zero
    /// or negative windows disable that policy dimension.
    pub(super) async fn run_retention_repo(
        &self,
        dry_run: bool,
        package_retention_days: i64,
        release_retention_days: i64,
        build_log_retention_days: i64,
    ) -> DeployServiceResult<RetentionRunResponse> {
        let mut packages_retired = 0_i64;
        let mut releases_retired = 0_i64;
        let mut build_logs_purged = 0_i64;

        if package_retention_days > 0 {
            // Packages past retention that no release references.
            let candidates = sqlx::query(
                "SELECT p.id FROM deploy_package p
                 WHERE p.deleted_at IS NULL
                   AND p.package_status IN ('READY', 'VALIDATED', 'SUPERSEDED')
                   AND p.updated_at < NOW() - ($1 * INTERVAL '1 day')
                   AND NOT EXISTS (
                       SELECT 1 FROM deploy_release r
                       WHERE r.package_id = p.id AND r.release_status IS NOT NULL
                   )",
            )
            .bind(package_retention_days)
            .fetch_all(&self.pool)
            .await
            .map_err(|error| store_error("scan package retention candidates", error))?;
            packages_retired = candidates.len() as i64;
            if !dry_run && !candidates.is_empty() {
                for candidate in &candidates {
                    let id: i64 = candidate.try_get("id").unwrap_or(0);
                    sqlx::query(
                        "UPDATE deploy_package SET package_status = 'RETIRED', updated_at = NOW(),
                             version = version + 1
                         WHERE id = $1 AND package_status <> 'RETIRED'",
                    )
                    .bind(id)
                    .execute(&self.pool)
                    .await
                    .map_err(|error| store_error("retire package", error))?;
                }
            }
        }

        if release_retention_days > 0 {
            // Releases past retention that no channel points at.
            let candidates = sqlx::query(
                "SELECT r.id FROM deploy_release r
                 WHERE r.deleted_at IS NULL
                   AND r.release_status IN ('ACTIVE', 'SUPERSEDED', 'DEPRECATED')
                   AND r.updated_at < NOW() - ($1 * INTERVAL '1 day')
                   AND NOT EXISTS (
                       SELECT 1 FROM deploy_release_channel c
                       WHERE c.current_release_id = r.id
                   )",
            )
            .bind(release_retention_days)
            .fetch_all(&self.pool)
            .await
            .map_err(|error| store_error("scan release retention candidates", error))?;
            releases_retired = candidates.len() as i64;
            if !dry_run && !candidates.is_empty() {
                for candidate in &candidates {
                    let id: i64 = candidate.try_get("id").unwrap_or(0);
                    sqlx::query(
                        "UPDATE deploy_release SET release_status = 'RETIRED', updated_at = NOW()
                         WHERE id = $1 AND release_status <> 'RETIRED'",
                    )
                    .bind(id)
                    .execute(&self.pool)
                    .await
                    .map_err(|error| store_error("retire release", error))?;
                }
            }
        }

        if build_log_retention_days > 0 {
            // Terminal builds past retention: drop the log reference; the
            // build row and audit trail are immutable and must survive.
            let candidates = sqlx::query(
                "SELECT id FROM deploy_build
                 WHERE deleted_at IS NULL
                   AND log_ref IS NOT NULL
                   AND build_status IN ('SUCCEEDED', 'FAILED', 'CANCELLED', 'TIMED_OUT')
                   AND updated_at < NOW() - ($1 * INTERVAL '1 day')",
            )
            .bind(build_log_retention_days)
            .fetch_all(&self.pool)
            .await
            .map_err(|error| store_error("scan build log retention candidates", error))?;
            build_logs_purged = candidates.len() as i64;
            if !dry_run && !candidates.is_empty() {
                for candidate in &candidates {
                    let id: i64 = candidate.try_get("id").unwrap_or(0);
                    sqlx::query(
                        "UPDATE deploy_build SET log_ref = NULL, updated_at = NOW(),
                             version = version + 1
                         WHERE id = $1 AND log_ref IS NOT NULL",
                    )
                    .bind(id)
                    .execute(&self.pool)
                    .await
                    .map_err(|error| store_error("purge build log reference", error))?;
                }
            }
        }

        Ok(RetentionRunResponse {
            dry_run,
            packages_retired,
            releases_retired,
            build_logs_purged,
            package_retention_days,
            release_retention_days,
            build_log_retention_days,
        })
    }

    /// Rebuilds the reconcilable daily usage aggregate from retained usage
    /// facts (design contract: `deploy_app_usage_daily` is rebuildable).
    /// Idempotent: the unique (tenant, app, date, dimension, unit) scope is
    /// upserted, never duplicated. `window_start`/`window_end` bound the
    /// rebuild; `None` rebuilds the trailing 90 days.
    ///
    /// Two rollups are written — the per-app one and the tenant-wide one — and
    /// `rebuilt_rows` counts the rows written by both, so it is the number of
    /// daily aggregates the window implies, not the number of facts.
    pub(super) async fn rebuild_usage_daily_repo(
        &self,
        window_start: Option<&str>,
        window_end: Option<&str>,
    ) -> DeployServiceResult<UsageReconciliationResponse> {
        let default_start = chrono::Utc::now()
            .checked_sub_signed(chrono::Duration::days(90))
            .map(|value| value.to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
            .unwrap_or_else(|| "2026-01-01T00:00:00.000Z".to_owned());
        let window_start = window_start.unwrap_or(&default_start).to_owned();
        let window_end = window_end.map(|value| value.to_owned()).unwrap_or_else(|| {
            chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
        });
        // Validate timestamps before binding to the interval.
        if chrono::DateTime::parse_from_rfc3339(&window_start).is_err()
            || chrono::DateTime::parse_from_rfc3339(&window_end).is_err()
        {
            return Err(DeployServiceError::validation(
                "windowStart/windowEnd must be RFC3339 timestamps",
            ));
        }
        if window_start >= window_end {
            return Err(DeployServiceError::validation(
                "windowStart must be before windowEnd",
            ));
        }
        // The daily aggregate is reduced here and written back with ids minted
        // by the repository's Snowflake generator, which is the only id
        // authority in this service. Generating ids in SQL would mint from a
        // second, unsynchronised space and break the global-uniqueness
        // invariant the rest of the schema relies on.
        let site_rows = self
            .rebuild_usage_daily_scope(&window_start, &window_end, UsageDailyScope::App)
            .await?;
        // Tenant-level daily rollup (SaaS billing): one row per tenant,
        // dimension and day, including unmanaged traffic (tenant 0).
        let tenant_rows = self
            .rebuild_usage_daily_scope(&window_start, &window_end, UsageDailyScope::Tenant)
            .await?;
        Ok(UsageReconciliationResponse {
            rebuilt_rows: site_rows + tenant_rows,
            window_start,
            window_end,
        })
    }

    /// Reduces `deploy_usage_event` into one daily row per
    /// (tenant, app, binding, date, dimension, unit) — or per
    /// (tenant, date, dimension, unit) for the tenant rollup — and upserts it.
    /// Returns the number of rows written.
    ///
    /// The caller's window bounds the batch, so the reduced rows are only as
    /// numerous as that window implies; this is an administrative maintenance
    /// sweep, not an interactive list.
    async fn rebuild_usage_daily_scope(
        &self,
        window_start: &str,
        window_end: &str,
        scope: UsageDailyScope,
    ) -> DeployServiceResult<i64> {
        // `binding_id` is grouped by its normalised value because the unique
        // index is declared on `COALESCE(binding_id, 0)`. Grouping on the raw
        // column would let a NULL row and a 0 row enter one batch as two rows
        // for the same conflict target, which PostgreSQL rejects outright with
        // "ON CONFLICT DO UPDATE command cannot affect row a second time".
        // `MIN` skips NULLs, so the stored value still round-trips the facts.
        //
        // The window bounds are cast explicitly: they are bound as RFC3339
        // strings and `period_start` is TIMESTAMPTZ, and PostgreSQL refuses to
        // compare the two without the cast.
        let (select_sql, insert_sql) = match scope {
            // App attribution is a hard requirement, not a nicety:
            // `deploy_app_usage_daily.app_id` is NOT NULL with a foreign key to
            // `deploy_app`, so a fact carrying no app cannot be stored here at
            // all — grouping on a NULL `app_id` would try to insert NULL and be
            // rejected. Facts without an app are unmanaged traffic (the
            // platform-attributed dimensions), and they belong to the tenant
            // rollup, which is the superset by design.
            UsageDailyScope::App => (
                "SELECT u.tenant_id,
                        MAX(u.organization_id) AS organization_id,
                        u.app_id,
                        MIN(u.binding_id) AS binding_id,
                        (u.period_start AT TIME ZONE 'UTC')::date AS usage_date,
                        u.dimension,
                        SUM(u.quantity)::bigint AS quantity,
                        u.unit,
                        'rebuild:' || to_char(MAX(u.ingested_at), 'YYYYMMDDHH24MISS') AS source_revision
                 FROM deploy_usage_event u
                 WHERE u.period_start >= CAST($1 AS TIMESTAMPTZ)
                   AND u.period_start < CAST($2 AS TIMESTAMPTZ)
                   AND u.app_id IS NOT NULL
                 GROUP BY u.tenant_id, u.app_id, COALESCE(u.binding_id, 0),
                          (u.period_start AT TIME ZONE 'UTC')::date, u.dimension, u.unit
                 ORDER BY u.tenant_id, u.app_id, COALESCE(u.binding_id, 0),
                          usage_date, u.dimension, u.unit",
                "INSERT INTO deploy_app_usage_daily
                    (id, uuid, tenant_id, organization_id, app_id, binding_id, usage_date,
                     dimension, quantity, unit, source_revision, finalization_status,
                     created_at, updated_at)
                 SELECT batch.id, gen_random_uuid(), batch.tenant_id, batch.organization_id,
                        batch.app_id, batch.binding_id, batch.usage_date, batch.dimension,
                        batch.quantity, batch.unit, batch.source_revision, 'PENDING',
                        NOW(), NOW()
                 FROM UNNEST(
                         $1::bigint[], $2::bigint[], $3::bigint[], $4::bigint[], $5::bigint[],
                         $6::date[], $7::text[], $8::bigint[], $9::text[], $10::text[]
                      ) AS batch(id, tenant_id, organization_id, app_id, binding_id,
                                 usage_date, dimension, quantity, unit, source_revision)
                 ON CONFLICT (tenant_id, app_id, COALESCE(binding_id, 0), usage_date, dimension, unit)
                 DO UPDATE SET quantity = EXCLUDED.quantity,
                               source_revision = EXCLUDED.source_revision,
                               -- A FINALIZED row is billing evidence, so it is
                               -- re-opened only when the reduced number actually
                               -- moved. Resetting the status unconditionally
                               -- would reopen a closed period on every rebuild;
                               -- never resetting it would let the rebuild
                               -- rewrite a number that has already been billed.
                               -- `finalization_status = 'PENDING'` is precisely
                               -- what the entitlement surface sums as not yet
                               -- settled.
                               finalization_status = CASE
                                   WHEN deploy_app_usage_daily.quantity IS DISTINCT FROM EXCLUDED.quantity
                                   THEN 'PENDING'
                                   ELSE deploy_app_usage_daily.finalization_status
                               END,
                               finalized_at = CASE
                                   WHEN deploy_app_usage_daily.quantity IS DISTINCT FROM EXCLUDED.quantity
                                   THEN NULL
                                   ELSE deploy_app_usage_daily.finalized_at
                               END,
                               updated_at = NOW()",
            ),
            UsageDailyScope::Tenant => (
                "SELECT u.tenant_id,
                        MAX(u.organization_id) AS organization_id,
                        (u.period_start AT TIME ZONE 'UTC')::date AS usage_date,
                        u.dimension,
                        SUM(u.quantity)::bigint AS quantity,
                        u.unit,
                        'rebuild:' || to_char(MAX(u.ingested_at), 'YYYYMMDDHH24MISS') AS source_revision
                 FROM deploy_usage_event u
                 WHERE u.period_start >= CAST($1 AS TIMESTAMPTZ)
                   AND u.period_start < CAST($2 AS TIMESTAMPTZ)
                 GROUP BY u.tenant_id, (u.period_start AT TIME ZONE 'UTC')::date,
                          u.dimension, u.unit
                 ORDER BY u.tenant_id, usage_date, u.dimension, u.unit",
                "INSERT INTO deploy_tenant_usage_daily
                    (id, uuid, tenant_id, organization_id, usage_date, dimension,
                     quantity, unit, source_revision, finalization_status, created_at, updated_at)
                 SELECT batch.id, gen_random_uuid(), batch.tenant_id, batch.organization_id,
                        batch.usage_date, batch.dimension, batch.quantity, batch.unit,
                        batch.source_revision, 'PENDING', NOW(), NOW()
                 FROM UNNEST(
                         $1::bigint[], $2::bigint[], $3::bigint[], $4::date[], $5::text[],
                         $6::bigint[], $7::text[], $8::text[]
                      ) AS batch(id, tenant_id, organization_id, usage_date, dimension,
                                 quantity, unit, source_revision)
                 ON CONFLICT (tenant_id, dimension, usage_date, unit)
                 DO UPDATE SET quantity = EXCLUDED.quantity,
                               source_revision = EXCLUDED.source_revision,
                               -- Same re-open rule as the app rollup above: the
                               -- status moves back to PENDING only when the
                               -- reduced quantity changed.
                               finalization_status = CASE
                                   WHEN deploy_tenant_usage_daily.quantity IS DISTINCT FROM EXCLUDED.quantity
                                   THEN 'PENDING'
                                   ELSE deploy_tenant_usage_daily.finalization_status
                               END,
                               finalized_at = CASE
                                   WHEN deploy_tenant_usage_daily.quantity IS DISTINCT FROM EXCLUDED.quantity
                                   THEN NULL
                                   ELSE deploy_tenant_usage_daily.finalized_at
                               END,
                               updated_at = NOW()",
            ),
        };

        let rows = sqlx::query(select_sql)
            .bind(window_start)
            .bind(window_end)
            .fetch_all(&self.pool)
            .await
            .map_err(|error| {
                store_error("reduce deploy_usage_event for the daily rebuild", error)
            })?;

        let app_scoped = matches!(scope, UsageDailyScope::App);
        let mut reduced = Vec::with_capacity(rows.len());
        for row in &rows {
            // Every non-optional column is read strictly. A decode failure must
            // surface as an error: defaulting it to 0/"" would write a value
            // that looks like reduced evidence but is not, and both target
            // tables are billing inputs.
            reduced.push(UsageDailyRow {
                tenant_id: row
                    .try_get("tenant_id")
                    .map_err(|error| store_error("map usage aggregate tenant", error))?,
                organization_id: row
                    .try_get("organization_id")
                    .map_err(|error| store_error("map usage aggregate organization", error))?,
                // `app_id` exists only in the app-scoped reduction, where the
                // `app_id IS NOT NULL` predicate guarantees a value.
                app_id: app_scoped
                    .then(|| {
                        row.try_get("app_id")
                            .map_err(|error| store_error("map usage aggregate app", error))
                    })
                    .transpose()?,
                // `binding_id` is the one genuinely optional dimension: it is
                // `MIN(binding_id)` over the facts grouped for this row.
                binding_id: app_scoped
                    .then(|| {
                        row.try_get::<Option<i64>, _>("binding_id")
                            .map_err(|error| store_error("map usage aggregate binding", error))
                    })
                    .transpose()?
                    .flatten(),
                usage_date: row
                    .try_get("usage_date")
                    .map_err(|error| store_error("map usage aggregate date", error))?,
                dimension: row
                    .try_get("dimension")
                    .map_err(|error| store_error("map usage aggregate dimension", error))?,
                quantity: row
                    .try_get("quantity")
                    .map_err(|error| store_error("map usage aggregate quantity", error))?,
                unit: row
                    .try_get("unit")
                    .map_err(|error| store_error("map usage aggregate unit", error))?,
                source_revision: row
                    .try_get("source_revision")
                    .map_err(|error| store_error("map usage aggregate revision", error))?,
            });
        }

        let ids = reduced
            .iter()
            .map(|_| next_id(self.id_generator()))
            .collect::<DeployServiceResult<Vec<i64>>>()?;
        let tenant_ids = reduced.iter().map(|row| row.tenant_id).collect::<Vec<_>>();
        let organization_ids = reduced
            .iter()
            .map(|row| row.organization_id)
            .collect::<Vec<_>>();
        let usage_dates = reduced.iter().map(|row| row.usage_date).collect::<Vec<_>>();
        let dimensions = reduced
            .iter()
            .map(|row| row.dimension.clone())
            .collect::<Vec<_>>();
        let quantities = reduced.iter().map(|row| row.quantity).collect::<Vec<_>>();
        let units = reduced
            .iter()
            .map(|row| row.unit.clone())
            .collect::<Vec<_>>();
        let source_revisions = reduced
            .iter()
            .map(|row| row.source_revision.clone())
            .collect::<Vec<_>>();

        let result = match scope {
            UsageDailyScope::App => {
                let app_ids = reduced.iter().map(|row| row.app_id).collect::<Vec<_>>();
                let binding_ids = reduced.iter().map(|row| row.binding_id).collect::<Vec<_>>();
                sqlx::query(insert_sql)
                    .bind(&ids)
                    .bind(&tenant_ids)
                    .bind(&organization_ids)
                    .bind(&app_ids)
                    .bind(&binding_ids)
                    .bind(&usage_dates)
                    .bind(&dimensions)
                    .bind(&quantities)
                    .bind(&units)
                    .bind(&source_revisions)
                    .execute(&self.pool)
                    .await
                    .map_err(|error| store_error("upsert deploy_app_usage_daily", error))?
            }
            UsageDailyScope::Tenant => sqlx::query(insert_sql)
                .bind(&ids)
                .bind(&tenant_ids)
                .bind(&organization_ids)
                .bind(&usage_dates)
                .bind(&dimensions)
                .bind(&quantities)
                .bind(&units)
                .bind(&source_revisions)
                .execute(&self.pool)
                .await
                .map_err(|error| store_error("upsert deploy_tenant_usage_daily", error))?,
        };
        Ok(result.rows_affected() as i64)
    }

    /// Backend signing identity health surface: expiry observations sorted by
    /// urgency, tenant-scoped when a tenant is provided.
    pub(super) async fn list_signing_identity_health_repo(
        &self,
        tenant_id: Option<i64>,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<SigningIdentityHealthPage> {
        let (page, page_size, offset) = pagination(page, page_size);
        // See `list_entitlement_projections_repo`: the tenant predicate and the
        // page window must not share a placeholder number, or `LIMIT $1` reads
        // the tenant id and `OFFSET $2` reads the page size.
        let (filter, paging, bind) = match tenant_id {
            Some(_tenant_id) => (
                "WHERE tenant_id = $1 AND deleted_at IS NULL",
                "LIMIT $2 OFFSET $3",
                true,
            ),
            None => ("WHERE deleted_at IS NULL", "LIMIT $1 OFFSET $2", false),
        };
        let count_query = format!("SELECT COUNT(*) AS total FROM deploy_signing_identity {filter}");
        let mut count = sqlx::query(AssertSqlSafe(&*count_query));
        if bind {
            count = count.bind(tenant_id.unwrap_or(0));
        }
        let count_row = count
            .fetch_one(&self.pool)
            .await
            .map_err(|error| store_error("count signing identities", error))?;
        let total: i64 = count_row.try_get("total").unwrap_or(0);

        let list_query = format!(
            "SELECT uuid, tenant_id, identity_name, signing_kind, expires_at, identity_status
             FROM deploy_signing_identity {filter}
             ORDER BY expires_at ASC NULLS LAST, id DESC {paging}"
        );
        let mut list = sqlx::query(AssertSqlSafe(&*list_query));
        if bind {
            list = list
                .bind(tenant_id.unwrap_or(0))
                .bind(page_size)
                .bind(offset);
        } else {
            list = list.bind(page_size).bind(offset);
        }
        let rows = list
            .fetch_all(&self.pool)
            .await
            .map_err(|error| store_error("list signing identity health", error))?;

        let now = chrono::Utc::now();
        let items = rows
            .iter()
            .map(|row| {
                let expires_at = optional_datetime(row, "expires_at")?;
                let days_until_expiry = expires_at.as_deref().and_then(|value| {
                    chrono::DateTime::parse_from_rfc3339(value)
                        .ok()
                        .map(|expiry| (expiry.with_timezone(&chrono::Utc) - now).num_days())
                });
                Ok(SigningIdentityHealthResponse {
                    id: row.try_get("uuid").unwrap_or_default(),
                    tenant_id: row.try_get("tenant_id").unwrap_or(0),
                    identity_name: row.try_get("identity_name").unwrap_or_default(),
                    signing_kind: row.try_get("signing_kind").unwrap_or_default(),
                    expires_at,
                    days_until_expiry,
                    identity_status: row.try_get("identity_status").unwrap_or_default(),
                })
            })
            .collect::<Result<Vec<_>, DeployServiceError>>()?;
        Ok(SigningIdentityHealthPage {
            items,
            total,
            page,
            page_size,
        })
    }
}
