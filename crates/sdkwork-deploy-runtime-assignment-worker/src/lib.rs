//! Bounded runtime-assignment outbox worker.

use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use sdkwork_intelligence_deploy_service::{
    RuntimePublicationBatchResult, RuntimePublicationService,
};
use tokio::time::{interval, MissedTickBehavior};

const DEFAULT_BATCH_SIZE: i64 = 50;
const DEFAULT_POLL_INTERVAL_MILLIS: u64 = 1_000;
const DEFAULT_LEASE_SECONDS: i64 = 30;
const MAXIMUM_BATCH_SIZE: i64 = 100;
const MINIMUM_POLL_INTERVAL_MILLIS: u64 = 100;
const MAXIMUM_POLL_INTERVAL_MILLIS: u64 = 60_000;
const MINIMUM_LEASE_SECONDS: i64 = 5;
const MAXIMUM_LEASE_SECONDS: i64 = 300;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeAssignmentWorkerConfig {
    pub worker_id: String,
    pub batch_size: i64,
    pub poll_interval: Duration,
    pub lease_seconds: i64,
}

impl RuntimeAssignmentWorkerConfig {
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
            return Err("runtime assignment worker id is invalid".to_owned());
        }
        if !(1..=MAXIMUM_BATCH_SIZE).contains(&batch_size) {
            return Err(
                "runtime assignment worker batch size must be between 1 and 100".to_owned(),
            );
        }
        if !(MINIMUM_POLL_INTERVAL_MILLIS..=MAXIMUM_POLL_INTERVAL_MILLIS)
            .contains(&poll_interval_millis)
        {
            return Err(
                "runtime assignment worker poll interval must be between 100 and 60000 milliseconds"
                    .to_owned(),
            );
        }
        if !(MINIMUM_LEASE_SECONDS..=MAXIMUM_LEASE_SECONDS).contains(&lease_seconds) {
            return Err(
                "runtime assignment worker lease must be between 5 and 300 seconds".to_owned(),
            );
        }
        Ok(Self {
            worker_id,
            batch_size,
            poll_interval: Duration::from_millis(poll_interval_millis),
            lease_seconds,
        })
    }

    pub fn from_env() -> Result<Self, String> {
        let worker_id = std::env::var("SDKWORK_DEPLOY_RUNTIME_ASSIGNMENT_WORKER_ID")
            .or_else(|_| std::env::var("SDKWORK_NODE_INSTANCE_ID"))
            .or_else(|_| {
                if sdkwork_deploy_core::deploy_is_production_like_environment() {
                    Err(std::env::VarError::NotPresent)
                } else {
                    Ok(format!("local-{}", std::process::id()))
                }
            })
            .map_err(|_| {
                "SDKWORK_NODE_INSTANCE_ID or SDKWORK_DEPLOY_RUNTIME_ASSIGNMENT_WORKER_ID is required in production-like environments"
                    .to_owned()
            })?;
        Self::new(
            worker_id,
            parse_env(
                "SDKWORK_DEPLOY_RUNTIME_ASSIGNMENT_BATCH_SIZE",
                DEFAULT_BATCH_SIZE,
            )?,
            parse_env(
                "SDKWORK_DEPLOY_RUNTIME_ASSIGNMENT_POLL_INTERVAL_MILLIS",
                DEFAULT_POLL_INTERVAL_MILLIS,
            )?,
            parse_env(
                "SDKWORK_DEPLOY_RUNTIME_ASSIGNMENT_LEASE_SECONDS",
                DEFAULT_LEASE_SECONDS,
            )?,
        )
    }
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

pub struct RuntimeAssignmentWorker {
    publication: Arc<RuntimePublicationService>,
    config: RuntimeAssignmentWorkerConfig,
}

impl RuntimeAssignmentWorker {
    pub fn new(
        publication: Arc<RuntimePublicationService>,
        config: RuntimeAssignmentWorkerConfig,
    ) -> Self {
        Self {
            publication,
            config,
        }
    }

    pub async fn run_once(&self) -> Result<RuntimePublicationBatchResult, String> {
        self.publication
            .publish_due(
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
        let mut ticker = interval(self.config.poll_interval);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
        tokio::pin!(shutdown);
        loop {
            tokio::select! {
                _ = &mut shutdown => {
                    tracing::info!(worker_id = %self.config.worker_id, "runtime assignment worker shutdown");
                    return;
                }
                _ = ticker.tick() => {}
            }
            match self.run_once().await {
                Ok(result)
                    if result.claimed > 0
                        || result.observations_checked > 0
                        || result.provider_event_assignments_checked > 0 =>
                {
                    tracing::info!(
                        worker_id = %self.config.worker_id,
                        claimed = result.claimed,
                        published = result.published,
                        failed = result.failed,
                        observations_checked = result.observations_checked,
                        observations_ingested = result.observations_ingested,
                        observations_pending = result.observations_pending,
                        observations_failed = result.observations_failed,
                        revisions_activated = result.revisions_activated,
                        provider_event_assignments_checked = result.provider_event_assignments_checked,
                        provider_event_assignments_failed = result.provider_event_assignments_failed,
                        provider_event_deliveries_ensured = result.provider_event_deliveries_ensured,
                        provider_event_deliveries_skipped = result.provider_event_deliveries_skipped,
                        "runtime assignment publication and observation batch completed"
                    );
                }
                Ok(_) => {}
                Err(error) => {
                    tracing::warn!(
                        worker_id = %self.config.worker_id,
                        error = %error,
                        "runtime assignment publication batch failed"
                    );
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
        assert!(RuntimeAssignmentWorkerConfig::new("worker-1".to_owned(), 50, 1_000, 30).is_ok());
        assert!(RuntimeAssignmentWorkerConfig::new("worker-1".to_owned(), 0, 1_000, 30).is_err());
        assert!(RuntimeAssignmentWorkerConfig::new("worker-1".to_owned(), 50, 10, 30).is_err());
        assert!(RuntimeAssignmentWorkerConfig::new("worker/1".to_owned(), 50, 1_000, 30).is_err());
    }
}
