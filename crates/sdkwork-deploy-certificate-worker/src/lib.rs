//! Certificate issuance executor: the half of the certificate lifecycle that
//! actually talks to a CA (PLAN-2026-0003 §9).
//!
//! The repository already accepts certificate intent and the renewal worker already
//! plans renewals, both of which stop at a `deploy_certificate_order` row. This
//! worker is what consumes those rows: it claims the ones that are ready, drives each
//! through the order state machine, asks the ACME engine to issue, validates the
//! returned material, and stores a version.
//!
//! Issuance lives in its own process, separate from planning, because the two have
//! nothing in common operationally. Planning is a burst of small database writes;
//! issuance is a multi-second conversation with a third party that can take minutes
//! and can fail in ways the CA, not this process, decided. Sharing a loop would let
//! a slow issuance delay the renewal planner, and would force one timeout to serve
//! both.
//!
//! Two rules are the reason this crate is more than a `loop { tick() }`:
//!
//! * **The lease is the fence.** Every order is claimed with an expiring lease, and
//!   every state transition is conditional on still holding it. A worker that stalls
//!   past its lease stops being able to act and *abandons* the order — it never fails
//!   it, because the order now belongs to somebody else and destroying their work
//!   would be worse than doing nothing.
//! * **The lease must outlast the CA conversation.** If it did not, a second worker
//!   would re-claim an order whose first worker was still mid-issuance, and the CA
//!   would be asked to issue twice for one intent — burning the tenant's rate-limit
//!   budget on a duplicate. That invariant is checked at startup rather than
//!   documented and hoped for.

use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use sdkwork_intelligence_deploy_service::{CertificateOrderBatchResult, DeployService};
use tokio::time::{interval, MissedTickBehavior};

const DEFAULT_BATCH_SIZE: i64 = 10;
const DEFAULT_POLL_INTERVAL_MILLIS: u64 = 15_000;
const DEFAULT_LEASE_SECONDS: i64 = 900;
const MAXIMUM_BATCH_SIZE: i64 = 25;
const MINIMUM_POLL_INTERVAL_MILLIS: u64 = 1_000;
const MAXIMUM_POLL_INTERVAL_MILLIS: u64 = 3_600_000;
const MINIMUM_LEASE_SECONDS: i64 = 30;
const MAXIMUM_LEASE_SECONDS: i64 = 1_800;
/// Mirrors `sdkwork_webserver_acme_service::DEFAULT_ACME_OPERATION_TIMEOUT_MS`.
///
/// Duplicated rather than imported so the worker does not have to link the ACME
/// engine — the only thing it needs from it is this one number, and the number is
/// part of the environment contract the deployment already sets.
const DEFAULT_ACME_OPERATION_TIMEOUT_MS: u64 = 180_000;
const MAXIMUM_ACME_OPERATION_TIMEOUT_MS: u64 = 600_000;

/// Bounded configuration for the issuance loop.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CertificateIssuanceWorkerConfig {
    pub worker_id: String,
    pub batch_size: i64,
    pub poll_interval: Duration,
    pub lease_seconds: i64,
}

impl CertificateIssuanceWorkerConfig {
    pub fn new(
        worker_id: String,
        batch_size: i64,
        poll_interval_millis: u64,
        lease_seconds: i64,
    ) -> Result<Self, String> {
        if worker_id.is_empty()
            || worker_id.len() > 128
            || !worker_id.bytes().all(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'-')
            })
        {
            return Err("certificate issuance worker id is invalid".to_owned());
        }
        // The same bounds the service enforces, checked here as well so a
        // misconfiguration fails at startup rather than on the first tick.
        if !(1..=MAXIMUM_BATCH_SIZE).contains(&batch_size) {
            return Err(format!(
                "certificate issuance worker batch size must be between 1 and {MAXIMUM_BATCH_SIZE}"
            ));
        }
        if !(MINIMUM_POLL_INTERVAL_MILLIS..=MAXIMUM_POLL_INTERVAL_MILLIS)
            .contains(&poll_interval_millis)
        {
            return Err(
                "certificate issuance worker poll interval must be between 1000 and 3600000 \
                 milliseconds"
                    .to_owned(),
            );
        }
        if !(MINIMUM_LEASE_SECONDS..=MAXIMUM_LEASE_SECONDS).contains(&lease_seconds) {
            return Err(format!(
                "certificate issuance worker lease must be between {MINIMUM_LEASE_SECONDS} and \
                 {MAXIMUM_LEASE_SECONDS} seconds"
            ));
        }
        Ok(Self {
            worker_id,
            batch_size,
            poll_interval: Duration::from_millis(poll_interval_millis),
            lease_seconds,
        })
    }

    pub fn from_env() -> Result<Self, String> {
        let config = Self::new(
            worker_id_from_env()?,
            parse_env(
                "SDKWORK_DEPLOY_CERTIFICATE_ISSUANCE_BATCH_SIZE",
                DEFAULT_BATCH_SIZE,
            )?,
            parse_env(
                "SDKWORK_DEPLOY_CERTIFICATE_ISSUANCE_POLL_INTERVAL_MILLIS",
                DEFAULT_POLL_INTERVAL_MILLIS,
            )?,
            parse_env(
                "SDKWORK_DEPLOY_CERTIFICATE_ISSUANCE_LEASE_SECONDS",
                DEFAULT_LEASE_SECONDS,
            )?,
        )?;
        // The one cross-argument invariant worth refusing to start over. A lease
        // shorter than the CA conversation is not a slow worker, it is a correctness
        // bug that only shows up as a duplicate ACME order under load.
        let operation_timeout_ms: u64 = parse_env(
            "SDKWORK_DEPLOY_ACME_OPERATION_TIMEOUT_MS",
            DEFAULT_ACME_OPERATION_TIMEOUT_MS,
        )?;
        let lease_millis = (config.lease_seconds as u128) * 1_000;
        if lease_millis <= u128::from(operation_timeout_ms) {
            return Err(format!(
                "certificate issuance lease ({} s) must exceed SDKWORK_DEPLOY_ACME_OPERATION_TIMEOUT_MS \
                 ({operation_timeout_ms} ms); otherwise a second worker re-claims an order while the \
                 first is still waiting on the CA",
                config.lease_seconds
            ));
        }
        // An operation timeout beyond the engine's own ceiling is a configuration
        // mistake, not something to clamp silently: the engine rejects it, and
        // failing here says so before any certificate request does.
        if operation_timeout_ms > MAXIMUM_ACME_OPERATION_TIMEOUT_MS {
            return Err(format!(
                "SDKWORK_DEPLOY_ACME_OPERATION_TIMEOUT_MS ({operation_timeout_ms}) exceeds the \
                 engine maximum of {MAXIMUM_ACME_OPERATION_TIMEOUT_MS} ms"
            ));
        }
        Ok(config)
    }
}

/// The worker's identity, which the database records as the lease holder.
///
/// A production-like environment must be told who it is: the lease is how one
/// replica knows another is already issuing a certificate, and two processes
/// sharing a generated name would each believe the other had crashed.
fn worker_id_from_env() -> Result<String, String> {
    std::env::var("SDKWORK_DEPLOY_CERTIFICATE_ISSUANCE_WORKER_ID")
        .or_else(|_| std::env::var("SDKWORK_NODE_INSTANCE_ID"))
        .or_else(|_| {
            if sdkwork_deploy_core::deploy_is_production_like_environment() {
                Err(std::env::VarError::NotPresent)
            } else {
                Ok(format!("local-{}", std::process::id()))
            }
        })
        .map_err(|_| {
            "SDKWORK_NODE_INSTANCE_ID or SDKWORK_DEPLOY_CERTIFICATE_ISSUANCE_WORKER_ID is required \
             in production-like environments"
                .to_owned()
        })
}

fn parse_env<T>(key: &str, default: T) -> Result<T, String>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    match std::env::var(key) {
        Ok(value) => value
            .parse::<T>()
            .map_err(|error| format!("invalid {key}: {error}")),
        Err(std::env::VarError::NotPresent) => Ok(default),
        Err(error) => Err(format!("read {key} failed: {error}")),
    }
}

pub struct CertificateIssuanceWorker {
    service: Arc<DeployService>,
    config: CertificateIssuanceWorkerConfig,
}

impl CertificateIssuanceWorker {
    pub fn new(service: Arc<DeployService>, config: CertificateIssuanceWorkerConfig) -> Self {
        Self { service, config }
    }

    /// Claims due orders and drives each to a stored version.
    pub async fn run_once(&self) -> Result<CertificateOrderBatchResult, String> {
        self.service
            .process_due_certificate_orders(
                &self.config.worker_id,
                self.config.batch_size,
                self.config.lease_seconds,
            )
            .await
            .map_err(|error| error.to_string())
    }

    pub async fn run_until_shutdown<F>(self, shutdown: F)
    where
        F: Future<Output = ()>,
    {
        // Refuse to run without an engine. Claiming is what charges an attempt and
        // takes a lease, so a worker that ticked anyway would take the whole backlog
        // and fail it — a missing configuration destroying certificate requests.
        // Exiting quietly is right here: the deployment simply does not issue, and
        // an orchestrator must not restart-loop over it.
        if !self.service.certificate_issuance_configured() {
            tracing::warn!(
                "no ACME engine is configured (SDKWORK_DEPLOY_CERTIFICATE_ISSUANCE); the \
                 certificate issuance worker has nothing to run and will not claim any order"
            );
            return;
        }
        let mut ticker = interval(self.config.poll_interval);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
        tokio::pin!(shutdown);
        loop {
            tokio::select! {
                _ = &mut shutdown => {
                    tracing::info!(
                        worker_id = %self.config.worker_id,
                        "certificate issuance worker shutdown"
                    );
                    return;
                }
                _ = ticker.tick() => {
                    match self.run_once().await {
                        // Silence when there is nothing to do: this loop runs for days
                        // between renewals, and a line every tick would bury the ticks
                        // that mattered.
                        Ok(result) if !result.is_idle() => {
                            tracing::info!(
                                worker_id = %self.config.worker_id,
                                claimed = result.claimed,
                                stored = result.stored,
                                failed = result.failed,
                                abandoned = result.abandoned,
                                "certificate issuance batch completed"
                            );
                        }
                        Ok(_) => {}
                        Err(error) => {
                            tracing::warn!(
                                worker_id = %self.config.worker_id,
                                error = %error,
                                "certificate issuance batch failed"
                            );
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worker_config_enforces_bounded_values() {
        assert!(
            CertificateIssuanceWorkerConfig::new("worker-1".to_owned(), 10, 15_000, 900).is_ok()
        );
        assert!(
            CertificateIssuanceWorkerConfig::new("worker/1".to_owned(), 10, 15_000, 900).is_err()
        );
        assert!(CertificateIssuanceWorkerConfig::new("".to_owned(), 10, 15_000, 900).is_err());
    }

    #[test]
    fn worker_config_rejects_values_the_service_would_reject() {
        // Bounds are duplicated on purpose, so this test is the one thing keeping
        // them from drifting: a worker that started happily and then failed every
        // tick would look like a database problem.
        for (batch, poll, lease) in [
            (0, 15_000, 900),
            (26, 15_000, 900),
            (10, 999, 900),
            (10, 3_600_001, 900),
            (10, 15_000, 29),
            (10, 15_000, 1_801),
        ] {
            assert!(
                CertificateIssuanceWorkerConfig::new("worker-1".to_owned(), batch, poll, lease)
                    .is_err(),
                "batch={batch} poll={poll} lease={lease} must be rejected"
            );
        }
    }

    #[test]
    fn default_lease_outlasts_the_engine_maximum_operation_timeout() {
        // The property the startup cross-check exists to protect, asserted directly
        // so a future change to the default cannot quietly break it.
        assert!(
            (DEFAULT_LEASE_SECONDS as u128) * 1_000 > u128::from(MAXIMUM_ACME_OPERATION_TIMEOUT_MS)
        );
    }
}
