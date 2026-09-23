//! Bounded certificate renewal worker: plans renewals before certificates
//! expire, and retires the ones whose window has already closed.
//!
//! Two cadences share one loop because they answer the same question at
//! different urgency. Planning has to be prompt — a renewal that starts late
//! only shows up when a certificate has already lapsed — while the expiry sweep
//! is housekeeping that writes rows, so running it at the planning cadence would
//! spend writes to change nothing.
//!
//! The worker itself holds no policy. Whether a certificate is due, how long to
//! back off after a refusal, and when an unrecoverable certificate stops being
//! retried all live in the service, so the console and the worker cannot disagree
//! about what "expiring soon" means. What this crate adds is the loop, the bounded
//! configuration, and the shutdown handling.

use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use sdkwork_intelligence_deploy_service::{
    CertificateRenewalBatchResult, DeployService, ExpiredCertificateSweep,
};
use tokio::time::{interval, MissedTickBehavior};

const DEFAULT_BATCH_SIZE: i64 = 50;
const DEFAULT_POLL_INTERVAL_MILLIS: u64 = 30_000;
const DEFAULT_LEASE_SECONDS: i64 = 300;
const DEFAULT_SWEEP_INTERVAL_MILLIS: u64 = 300_000;
const MAXIMUM_BATCH_SIZE: i64 = 100;
const MINIMUM_POLL_INTERVAL_MILLIS: u64 = 1_000;
const MAXIMUM_POLL_INTERVAL_MILLIS: u64 = 3_600_000;
const MINIMUM_LEASE_SECONDS: i64 = 30;
const MAXIMUM_LEASE_SECONDS: i64 = 1_800;
const MINIMUM_SWEEP_INTERVAL_MILLIS: u64 = 60_000;
const MAXIMUM_SWEEP_INTERVAL_MILLIS: u64 = 86_400_000;

/// Bounded configuration for the renewal loop.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CertificateRenewalWorkerConfig {
    pub worker_id: String,
    pub batch_size: i64,
    pub poll_interval: Duration,
    pub lease_seconds: i64,
    pub sweep_interval: Duration,
}

impl CertificateRenewalWorkerConfig {
    pub fn new(
        worker_id: String,
        batch_size: i64,
        poll_interval_millis: u64,
        lease_seconds: i64,
        sweep_interval_millis: u64,
    ) -> Result<Self, String> {
        if worker_id.is_empty()
            || worker_id.len() > 128
            || !worker_id.bytes().all(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'-')
            })
        {
            return Err("certificate renewal worker id is invalid".to_owned());
        }
        // The same bound the service enforces, checked here as well so a
        // misconfiguration fails at startup rather than on the first tick.
        if !(1..=MAXIMUM_BATCH_SIZE).contains(&batch_size) {
            return Err(
                "certificate renewal worker batch size must be between 1 and 100".to_owned(),
            );
        }
        if !(MINIMUM_POLL_INTERVAL_MILLIS..=MAXIMUM_POLL_INTERVAL_MILLIS)
            .contains(&poll_interval_millis)
        {
            return Err(
                "certificate renewal worker poll interval must be between 1000 and 3600000 milliseconds"
                    .to_owned(),
            );
        }
        if !(MINIMUM_LEASE_SECONDS..=MAXIMUM_LEASE_SECONDS).contains(&lease_seconds) {
            return Err(
                "certificate renewal worker lease must be between 30 and 1800 seconds".to_owned(),
            );
        }
        if !(MINIMUM_SWEEP_INTERVAL_MILLIS..=MAXIMUM_SWEEP_INTERVAL_MILLIS)
            .contains(&sweep_interval_millis)
        {
            return Err(
                "certificate renewal worker sweep interval must be between 60000 and 86400000 milliseconds"
                    .to_owned(),
            );
        }
        Ok(Self {
            worker_id,
            batch_size,
            poll_interval: Duration::from_millis(poll_interval_millis),
            lease_seconds,
            sweep_interval: Duration::from_millis(sweep_interval_millis),
        })
    }

    pub fn from_env() -> Result<Self, String> {
        Self::new(
            worker_id_from_env()?,
            parse_env(
                "SDKWORK_DEPLOY_CERTIFICATE_RENEWAL_BATCH_SIZE",
                DEFAULT_BATCH_SIZE,
            )?,
            parse_env(
                "SDKWORK_DEPLOY_CERTIFICATE_RENEWAL_POLL_INTERVAL_MILLIS",
                DEFAULT_POLL_INTERVAL_MILLIS,
            )?,
            parse_env(
                "SDKWORK_DEPLOY_CERTIFICATE_RENEWAL_LEASE_SECONDS",
                DEFAULT_LEASE_SECONDS,
            )?,
            parse_env(
                "SDKWORK_DEPLOY_CERTIFICATE_RENEWAL_SWEEP_INTERVAL_MILLIS",
                DEFAULT_SWEEP_INTERVAL_MILLIS,
            )?,
        )
    }
}

/// The worker's identity, which the database records as the lease holder.
///
/// A production-like environment must be told who it is: the lease is how one
/// replica knows another is already renewing a certificate, and two processes
/// sharing a generated name would each believe the other had crashed.
fn worker_id_from_env() -> Result<String, String> {
    std::env::var("SDKWORK_DEPLOY_CERTIFICATE_RENEWAL_WORKER_ID")
        .or_else(|_| std::env::var("SDKWORK_NODE_INSTANCE_ID"))
        .or_else(|_| {
            if sdkwork_deploy_core::deploy_is_production_like_environment() {
                Err(std::env::VarError::NotPresent)
            } else {
                Ok(format!("local-{}", std::process::id()))
            }
        })
        .map_err(|_| {
            "SDKWORK_NODE_INSTANCE_ID or SDKWORK_DEPLOY_CERTIFICATE_RENEWAL_WORKER_ID is required \
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

pub struct CertificateRenewalWorker {
    service: Arc<DeployService>,
    config: CertificateRenewalWorkerConfig,
}

impl CertificateRenewalWorker {
    pub fn new(service: Arc<DeployService>, config: CertificateRenewalWorkerConfig) -> Self {
        Self { service, config }
    }

    /// Claims due certificates and opens a renewal order for each.
    pub async fn run_once(&self) -> Result<CertificateRenewalBatchResult, String> {
        self.service
            .plan_due_certificate_renewals(
                &self.config.worker_id,
                self.config.batch_size,
                self.config.lease_seconds,
            )
            .await
            .map_err(|error| error.to_string())
    }

    /// Retires certificates and versions whose validity window has closed.
    pub async fn sweep_once(&self) -> Result<ExpiredCertificateSweep, String> {
        self.service
            .sweep_expired_certificates(self.config.batch_size)
            .await
            .map_err(|error| error.to_string())
    }

    pub async fn run_until_shutdown<F>(self, shutdown: F)
    where
        F: Future<Output = ()>,
    {
        let mut renewal_ticker = interval(self.config.poll_interval);
        renewal_ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
        let mut sweep_ticker = interval(self.config.sweep_interval);
        sweep_ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
        tokio::pin!(shutdown);
        // 停机排空语义:select 只决定"下一个要执行的动作",动作体在
        // select 之外完整执行——在途批次不再被停机信号丢弃。
        enum WorkerAction {
            Renew,
            Sweep,
        }
        loop {
            let action = tokio::select! {
                _ = &mut shutdown => {
                    tracing::info!(
                        worker_id = %self.config.worker_id,
                        "certificate renewal worker shutdown"
                    );
                    return;
                }
                _ = renewal_ticker.tick() => WorkerAction::Renew,
                _ = sweep_ticker.tick() => WorkerAction::Sweep,
            };
            match action {
                WorkerAction::Renew => match self.run_once().await {
                    // Silence when there is nothing to do: this loop runs for
                    // days between renewals, and a line every tick would bury
                    // the ticks that mattered.
                    Ok(result) if !result.is_idle() => {
                        tracing::info!(
                            worker_id = %self.config.worker_id,
                            claimed = result.claimed,
                            ordered = result.ordered,
                            failed = result.failed,
                            "certificate renewal batch completed"
                        );
                    }
                    Ok(_) => {}
                    Err(error) => {
                        tracing::warn!(
                            worker_id = %self.config.worker_id,
                            error = %error,
                            "certificate renewal batch failed"
                        );
                    }
                },
                WorkerAction::Sweep => match self.sweep_once().await {
                    Ok(sweep) if !sweep.is_empty() => {
                        tracing::info!(
                            worker_id = %self.config.worker_id,
                            certificates_expired = sweep.certificates_expired,
                            versions_expired = sweep.versions_expired,
                            renewals_cancelled = sweep.renewals_cancelled,
                            "certificate expiry sweep completed"
                        );
                    }
                    Ok(_) => {}
                    Err(error) => {
                        tracing::warn!(
                            worker_id = %self.config.worker_id,
                            error = %error,
                            "certificate expiry sweep failed"
                        );
                    }
                },
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worker_config_enforces_bounded_values() {
        assert!(CertificateRenewalWorkerConfig::new(
            "worker-1".to_owned(),
            50,
            30_000,
            300,
            300_000
        )
        .is_ok());
        assert!(CertificateRenewalWorkerConfig::new(
            "worker/1".to_owned(),
            50,
            30_000,
            300,
            300_000
        )
        .is_err());
        assert!(
            CertificateRenewalWorkerConfig::new("".to_owned(), 50, 30_000, 300, 300_000).is_err()
        );
    }

    #[test]
    fn worker_config_rejects_values_the_service_would_reject() {
        // Bounds are duplicated on purpose, so this test is the one thing keeping
        // them from drifting: a worker that started happily and then failed every
        // tick would look like a database problem.
        assert!(CertificateRenewalWorkerConfig::new(
            "worker-1".to_owned(),
            0,
            30_000,
            300,
            300_000
        )
        .is_err());
        assert!(CertificateRenewalWorkerConfig::new(
            "worker-1".to_owned(),
            101,
            30_000,
            300,
            300_000
        )
        .is_err());
        assert!(
            CertificateRenewalWorkerConfig::new("worker-1".to_owned(), 50, 999, 300, 300_000)
                .is_err()
        );
        assert!(CertificateRenewalWorkerConfig::new(
            "worker-1".to_owned(),
            50,
            30_000,
            29,
            300_000
        )
        .is_err());
        assert!(CertificateRenewalWorkerConfig::new(
            "worker-1".to_owned(),
            50,
            30_000,
            1_801,
            300_000
        )
        .is_err());
        assert!(CertificateRenewalWorkerConfig::new(
            "worker-1".to_owned(),
            50,
            30_000,
            300,
            59_999
        )
        .is_err());
    }
}
