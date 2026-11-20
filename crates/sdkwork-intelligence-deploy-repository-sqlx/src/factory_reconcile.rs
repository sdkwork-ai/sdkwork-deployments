//! Factory reconciliation read model.
//!
//! `APPLICATION_FACTORY_DEPLOYMENT_SPEC.md` section 9 requires reconciliation for
//! every soft cross-module reference: "deploy application `identity_status` vs
//! installation existence, carrier `deploy_application_id` vs deploy inventory,
//! installation environment domains vs carrier hosts, and token anchor vs
//! installation. A dangling reference is an alert, not a valid state."
//!
//! Nothing performed those checks before this module. The deploy plane owns the
//! anchor (`deploy_app.id`), so it is the natural read point; IAM and the
//! hosting carrier live in sibling modules, and this module reads them
//! **defensively**: a schema that does not host the sibling table is reported as
//! `unavailable` instead of failing the whole report, exactly as
//! `scaffold_market.rs` probes `to_regclass` before joining commerce tables.
//!
//! The report is read-only. It never repairs anything: a dangling link is an
//! operator decision (retry provisioning, re-create the carrier, or retire the
//! application), and a silent auto-repair would be another authority deciding
//! what the link should be.

use sqlx::{PgPool, Row};

use sdkwork_deploy_contract::{
    AppFactoryReconciliation, DeployServiceResult, FactoryDanglingReference,
};

use crate::support::store_error;
use crate::DeployRepository;

impl DeployRepository {
    /// Public read-only reconciliation entry point.
    ///
    /// The HTTP surface calls the service (which requires a tenant context); the
    /// factory reconciliation worker owns the long-running loop and calls this
    /// directly, so the report has exactly one implementation.
    pub async fn reconcile_factory_references(
        &self,
        tenant_id: i64,
    ) -> DeployServiceResult<AppFactoryReconciliation> {
        self.reconcile_factory_references_repo(tenant_id).await
    }

    /// Reconciles the factory's soft cross-module references for one tenant.
    pub(super) async fn reconcile_factory_references_repo(
        &self,
        tenant_id: i64,
    ) -> DeployServiceResult<AppFactoryReconciliation> {
        let mut status = "CLEAN";
        let mut dangling: Vec<FactoryDanglingReference> = Vec::new();
        let mut unavailable: Vec<String> = Vec::new();
        let has_iam = table_exists(&self.pool, "iam_application_installation").await?;
        let has_carrier = table_exists(&self.pool, "webserver_application").await?;
        // Record the degraded authorities up front, not inside the per-app loop: a
        // tenant with no applications still needs to know that the IAM and carrier
        // checks never ran.
        if !has_iam {
            unavailable.push("iam: iam_application_installation is not in this schema".to_owned());
        }
        if !has_carrier {
            unavailable.push("webserver: webserver_application is not in this schema".to_owned());
        }

        let apps = sqlx::query(
            "SELECT id, uuid, identity_status, uninstall_status
             FROM deploy_app
             WHERE tenant_id = $1 AND deleted_at IS NULL
             ORDER BY id",
        )
        .bind(tenant_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error("list deploy_app for reconciliation", error))?;
        let apps_checked = apps.len().to_string();

        for app in &apps {
            let app_uuid: String = app.try_get("uuid").unwrap_or_default();
            let app_id: i64 = app.try_get("id").unwrap_or_default();
            let identity_status: String = app.try_get("identity_status").unwrap_or_default();
            let uninstall_status: String = app.try_get("uninstall_status").unwrap_or_default();

            // (a) Identity: `active` requires a live installation.
            if has_iam {
                let installation = sqlx::query(
                    "SELECT id, status, provisioning_status
                     FROM iam_application_installation
                     WHERE tenant_id = $1::text AND deploy_application_id = $2::text
                     LIMIT 1",
                )
                .bind(tenant_id.to_string())
                .bind(app_id.to_string())
                .fetch_optional(&self.pool)
                .await
                .map_err(|error| store_error("read installation for reconciliation", error))?;
                match (identity_status.as_str(), installation.as_ref()) {
                    ("active", None) => dangling.push(FactoryDanglingReference {
                        authority: "iam".to_owned(),
                        app_id: Some(app_uuid.clone()),
                        deploy_application_id: Some(app_id.to_string()),
                        code: "IDENTITY_ACTIVE_WITHOUT_INSTALLATION".to_owned(),
                        detail: format!(
                            "deploy application {app_id} reports identity_status=active but IAM has no installation bound to it"
                        ),
                    }),
                    ("pending_identity" | "failed", Some(row))
                        if row.try_get::<String, _>("status").unwrap_or_default() == "enabled" =>
                    {
                        dangling.push(FactoryDanglingReference {
                            authority: "iam".to_owned(),
                            app_id: Some(app_uuid.clone()),
                            deploy_application_id: Some(app_id.to_string()),
                            code: "INSTALLATION_ENABLED_BUT_DEPLOY_PENDING".to_owned(),
                            detail: format!(
                                "deploy application {app_id} reports identity_status={identity_status} but its IAM installation is enabled"
                            ),
                        });
                    }
                    _ => {}
                }

                // (b) Uninstall ordering: a completed uninstall must not leave an
                // installation able to issue credentials.
                if uninstall_status == "complete" {
                    if let Some(row) = installation.as_ref() {
                        let status: String = row.try_get("status").unwrap_or_default();
                        if status.eq_ignore_ascii_case("enabled") {
                            dangling.push(FactoryDanglingReference {
                                authority: "iam".to_owned(),
                                app_id: Some(app_uuid.clone()),
                                deploy_application_id: Some(app_id.to_string()),
                                code: "UNINSTALLED_INSTALLATION_STILL_ENABLED".to_owned(),
                                detail: format!(
                                    "deploy application {app_id} is uninstalled but its IAM installation is still enabled; issuance has not stopped"
                                ),
                            });
                        }
                    }
                }

                // (c) Installation environment domains vs the deploy domain plan.
                let domains = sqlx::query(
                    "SELECT e.env_key, e.primary_domain
                     FROM iam_application_installation_environment e
                     JOIN iam_application_installation a ON a.id = e.installation_id
                     WHERE a.tenant_id = $1::text AND a.deploy_application_id = $2::text
                       AND e.deleted_at IS NULL AND e.primary_domain IS NOT NULL
                       AND e.env_status <> 'disabled'",
                )
                .bind(tenant_id.to_string())
                .bind(app_id.to_string())
                .fetch_all(&self.pool)
                .await
                .map_err(|error| {
                    store_error("read installation domains for reconciliation", error)
                })?;
                for domain in &domains {
                    let hostname: String = domain.try_get("primary_domain").unwrap_or_default();
                    let planned: Option<String> = sqlx::query_scalar(
                        "SELECT hostname_ascii FROM deploy_app_binding
                         WHERE tenant_id = $1 AND app_id = $2
                           AND lower(hostname_ascii) = lower($3)
                           AND deleted_at IS NULL AND status <> 'DISABLED'
                         LIMIT 1",
                    )
                    .bind(tenant_id)
                    .bind(app_id)
                    .bind(&hostname)
                    .fetch_optional(&self.pool)
                    .await
                    .map_err(|error| {
                        store_error("check planned hostname for reconciliation", error)
                    })?;
                    if planned.is_none() {
                        dangling.push(FactoryDanglingReference {
                            authority: "iam".to_owned(),
                            app_id: Some(app_uuid.clone()),
                            deploy_application_id: Some(app_id.to_string()),
                            code: "INSTALLATION_DOMAIN_NOT_PLANNED".to_owned(),
                            detail: format!(
                                "installation environment {} registers domain {hostname}, but deploy has no planned hostname for it",
                                domain
                                    .try_get::<String, _>("env_key")
                                    .unwrap_or_default()
                            ),
                        });
                    }
                }
            } else if !unavailable.iter().any(|entry| entry == "iam") {
                unavailable
                    .push("iam: iam_application_installation is not in this schema".to_owned());
            }

            // (d) One live carrier per deploy application.
            if has_carrier {
                let carriers: Option<i64> = sqlx::query_scalar(
                    "SELECT COUNT(*) FROM webserver_application
                     WHERE deploy_application_id = $1 AND deleted_at IS NULL AND tenant_id = $2",
                )
                .bind(app_id)
                .bind(tenant_id)
                .fetch_optional(&self.pool)
                .await
                .map_err(|error| store_error("count carrier rows for reconciliation", error))?;
                let carriers = carriers.unwrap_or(0);
                if carriers > 1 {
                    dangling.push(FactoryDanglingReference {
                        authority: "webserver".to_owned(),
                        app_id: Some(app_uuid.clone()),
                        deploy_application_id: Some(app_id.to_string()),
                        code: "MULTIPLE_CARRIERS_FOR_ONE_APPLICATION".to_owned(),
                        detail: format!(
                            "deploy application {app_id} is anchored by {carriers} live carriers; the 1:1 rule allows one"
                        ),
                    });
                }
            } else if !unavailable
                .iter()
                .any(|entry| entry.starts_with("webserver"))
            {
                unavailable
                    .push("webserver: webserver_application is not in this schema".to_owned());
            }
        }

        // (e) Carrier-side orphans: a carrier pointing at an anchor no deploy
        // application owns.
        if has_carrier {
            let orphans = sqlx::query(
                "SELECT a.deploy_application_id, a.uuid
                 FROM webserver_application a
                 LEFT JOIN deploy_app d ON d.id = a.deploy_application_id AND d.deleted_at IS NULL
                 WHERE a.deleted_at IS NULL AND a.tenant_id = $1
                   AND a.deploy_application_id IS NOT NULL
                   AND d.id IS NULL
                 LIMIT 100",
            )
            .bind(tenant_id)
            .fetch_all(&self.pool)
            .await
            .map_err(|error| store_error("find orphan carriers for reconciliation", error))?;
            for orphan in &orphans {
                dangling.push(FactoryDanglingReference {
                    authority: "webserver".to_owned(),
                    app_id: orphan.try_get::<String, _>("uuid").ok(),
                    deploy_application_id: orphan
                        .try_get::<i64, _>("deploy_application_id")
                        .ok()
                        .map(|value| value.to_string()),
                    code: "CARRIER_WITHOUT_DEPLOY_APPLICATION".to_owned(),
                    detail: format!(
                        "carrier {} points at deploy application {} which no live deploy_app row owns",
                        orphan.try_get::<String, _>("uuid").unwrap_or_default(),
                        orphan
                            .try_get::<i64, _>("deploy_application_id")
                            .map(|value| value.to_string())
                            .unwrap_or_default()
                    ),
                });
            }
        }

        // (f) The token anchor lives in client profile files, not in this
        // database; the file-side check is `check-release-access-token-env.mjs`,
        // which the packaging gate already runs. It is not listed under
        // `unavailable`: that field means "a check that should have run could not",
        // and a deliberately tool-side check is not a degraded authority.
        if !dangling.is_empty() {
            status = "DRIFT";
        } else if !unavailable.is_empty() {
            status = "PARTIAL";
        }
        Ok(AppFactoryReconciliation {
            status: status.to_owned(),
            apps_checked,
            dangling,
            unavailable,
        })
    }
}

/// Whether a table exists in the current schema, without assuming the sibling
/// module is deployed alongside deploy.
async fn table_exists(pool: &PgPool, table: &str) -> DeployServiceResult<bool> {
    // Schema-qualified on purpose: an unqualified probe resolves through
    // `search_path`, so a sibling table left in `public` by another test (or
    // another module's schema) would make this schema look like it hosts the
    // authority and turn a degraded report into a false clean one.
    let exists: Option<bool> =
        sqlx::query_scalar("SELECT to_regclass(format('%I.%I', current_schema(), $1)) IS NOT NULL")
            .bind(table)
            .fetch_optional(pool)
            .await
            .map_err(|error| store_error("probe cross-module table", error))?;
    Ok(exists.unwrap_or(false))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_report_with_findings_is_drift_and_names_the_authority() {
        let report = AppFactoryReconciliation {
            status: "DRIFT".to_owned(),
            apps_checked: "1".to_owned(),
            dangling: vec![FactoryDanglingReference {
                authority: "iam".to_owned(),
                app_id: Some("app-uuid".to_owned()),
                deploy_application_id: Some("42".to_owned()),
                code: "IDENTITY_ACTIVE_WITHOUT_INSTALLATION".to_owned(),
                detail: "detail".to_owned(),
            }],
            unavailable: Vec::new(),
        };
        let json = serde_json::to_value(&report).expect("serializes");
        assert_eq!(json["status"], "DRIFT");
        assert_eq!(json["dangling"][0]["authority"], "iam");
        assert_eq!(json["dangling"][0]["deployApplicationId"], "42");
        assert_eq!(json["appsChecked"], "1");
    }

    #[test]
    fn an_unavailable_authority_is_visible_instead_of_being_hidden() {
        let report = AppFactoryReconciliation {
            status: "PARTIAL".to_owned(),
            apps_checked: "0".to_owned(),
            dangling: Vec::new(),
            unavailable: vec!["webserver".to_owned()],
        };
        let json = serde_json::to_value(&report).expect("serializes");
        assert_eq!(json["status"], "PARTIAL");
        assert_eq!(json["unavailable"][0], "webserver");
    }
}
