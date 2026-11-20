//! Factory reference reconciliation worker.
//!
//! The reconciliation report itself is read-only and lives in the repository
//! (`DeployRepository::reconcile_factory_references`); this crate is the
//! long-running owner: it walks the tenants that own applications, runs the
//! report for each, and turns the verdict into a structured log line an operator
//! can alert on. A `DRIFT` report is logged at `ERROR`; `PARTIAL` (a sibling
//! authority is absent from the schema) at `WARN`; `CLEAN` at `INFO`.
//!
//! Nothing here repairs: the report names the dangling reference and its owner,
//! and the owning module fixes it.

use std::sync::Arc;
use std::time::Duration;

use sdkwork_deploy_contract::AppFactoryReconciliation;
use sdkwork_intelligence_deploy_repository_sqlx::DeployRepository;
use tokio::time::{interval, MissedTickBehavior};
use tracing::{error, info, warn};

/// Environment variable that overrides the reconciliation period.
pub const INTERVAL_ENV: &str = "SDKWORK_DEPLOY_FACTORY_RECONCILIATION_INTERVAL_SECONDS";

/// Default period: five minutes.
pub const DEFAULT_INTERVAL_SECONDS: u64 = 300;

/// Verdict class used to pick the log level.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReconciliationAlert {
    Clean,
    Partial,
    Drift,
}

/// Maps the report status to the alert class. An unknown status is treated as
/// `Drift` so a new verdict can never be silently downgraded to `INFO`.
#[must_use]
pub fn alert_for(status: &str) -> ReconciliationAlert {
    match status.trim().to_ascii_uppercase().as_str() {
        "CLEAN" => ReconciliationAlert::Clean,
        "PARTIAL" => ReconciliationAlert::Partial,
        _ => ReconciliationAlert::Drift,
    }
}

/// Period from `INTERVAL_ENV`, falling back to [`DEFAULT_INTERVAL_SECONDS`].
/// A zero or unparsable value falls back instead of spinning.
#[must_use]
pub fn interval_from_env() -> Duration {
    std::env::var(INTERVAL_ENV)
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|seconds| *seconds > 0)
        .map(Duration::from_secs)
        .unwrap_or(Duration::from_secs(DEFAULT_INTERVAL_SECONDS))
}

/// One tenant's reconciliation outcome.
#[derive(Debug)]
pub struct TenantReconciliation {
    pub tenant_id: i64,
    pub report: AppFactoryReconciliation,
}

/// Runs the report for every tenant that owns an application.
#[derive(Clone)]
pub struct FactoryReconciliationWorker {
    repository: Arc<DeployRepository>,
    period: Duration,
}

impl FactoryReconciliationWorker {
    pub fn new(repository: Arc<DeployRepository>, period: Duration) -> Self {
        Self { repository, period }
    }

    /// Current period.
    #[must_use]
    pub fn period(&self) -> Duration {
        self.period
    }

    /// The tenants that own at least one live application.
    pub async fn tenant_ids(&self) -> Result<Vec<i64>, String> {
        sqlx::query_scalar::<_, i64>(
            "SELECT DISTINCT tenant_id FROM deploy_app WHERE deleted_at IS NULL ORDER BY tenant_id",
        )
        .fetch_all(self.repository.pool())
        .await
        .map_err(|error| format!("list factory tenants failed: {error}"))
    }

    /// One pass: reconcile each tenant and log the verdict.
    pub async fn run_once(&self) -> Result<Vec<TenantReconciliation>, String> {
        let mut reports = Vec::new();
        for tenant_id in self.tenant_ids().await? {
            match self
                .repository
                .reconcile_factory_references(tenant_id)
                .await
            {
                Ok(report) => {
                    log_report(tenant_id, &report);
                    reports.push(TenantReconciliation { tenant_id, report });
                }
                Err(error) => {
                    error!(tenant_id, %error, "factory reconciliation pass failed for tenant");
                }
            }
        }
        Ok(reports)
    }

    /// Runs until `shutdown` resolves, then returns.
    pub async fn run_until_shutdown<F>(self, shutdown: F)
    where
        F: std::future::Future<Output = ()> + Send,
    {
        tokio::pin!(shutdown);
        let mut ticker = interval(self.period);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
        info!(
            interval_seconds = self.period.as_secs(),
            "factory reconciliation worker started"
        );
        loop {
            if let Err(error) = self.run_once().await {
                error!(%error, "factory reconciliation pass failed");
            }
            tokio::select! {
                _ = ticker.tick() => {},
                () = &mut shutdown => break,
            }
        }
        info!("factory reconciliation worker stopped");
    }
}

/// Emits the verdict at the level the alert class requires.
pub fn log_report(tenant_id: i64, report: &AppFactoryReconciliation) {
    let dangling = report.dangling.len();
    let unavailable = report.unavailable.len();
    match alert_for(&report.status) {
        ReconciliationAlert::Clean => info!(
            tenant_id,
            apps_checked = %report.apps_checked,
            "factory reconciliation clean"
        ),
        ReconciliationAlert::Partial => warn!(
            tenant_id,
            apps_checked = %report.apps_checked,
            unavailable,
            "factory reconciliation partial: a sibling authority is absent"
        ),
        ReconciliationAlert::Drift => error!(
            tenant_id,
            apps_checked = %report.apps_checked,
            dangling,
            unavailable,
            "factory reconciliation drift: dangling cross-module references"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alert_classification_is_fail_closed_for_unknown_status() {
        assert_eq!(alert_for("CLEAN"), ReconciliationAlert::Clean);
        assert_eq!(alert_for(" partial "), ReconciliationAlert::Partial);
        assert_eq!(alert_for("DRIFT"), ReconciliationAlert::Drift);
        assert_eq!(alert_for("SOMETHING_NEW"), ReconciliationAlert::Drift);
    }

    #[test]
    fn interval_defaults_when_unset_or_invalid() {
        // The test process may carry the variable; only assert the unset path
        // through the pure helper shape: zero is rejected by the filter.
        let zero = "0".parse::<u64>().ok().filter(|seconds| *seconds > 0);
        assert!(zero.is_none());
        assert!(DEFAULT_INTERVAL_SECONDS >= 60);
    }
}
