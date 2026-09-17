//! Durable runtime assignment publication orchestration.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::{DateTime, Duration, SecondsFormat, Utc};
use sdkwork_deploy_content_provider_port::{
    NoopWebsiteProviderEventDeliveryPort, WebsiteProviderEventDeliveryPort,
    WebsiteProviderEventDeliveryResult,
};
use sdkwork_deploy_contract::{DeployServiceError, DeployServiceErrorKind, DeployServiceResult};
use sdkwork_deploy_runtime_compiler::{
    canonical_sha256_excluding_field, compile_runtime_set, normalize_runtime_descriptors,
    runtime_set_size_bytes, CompiledRuntimeSet, RuntimeEnvironment, RuntimeSetCompilationInput,
};
use sdkwork_deploy_web_port::{
    DeployWebRuntimePort, RuntimeAssignmentReceipt, RuntimeObservationReceipt,
};
use serde_json::Value;

const DEFAULT_MAXIMUM_SITES: usize = 10_000;
const MAXIMUM_PUBLICATION_BATCH: i64 = 100;
const MAXIMUM_ATTEMPTS: i32 = 20;
const MINIMUM_LEASE_SECONDS: i64 = 5;
const MAXIMUM_LEASE_SECONDS: i64 = 300;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeTarget {
    pub target_uuid: String,
    pub tenant_id: i64,
    pub node_uuid: String,
    pub environment: RuntimeEnvironment,
    pub tenant_scope_hash: String,
}

#[derive(Clone, Debug)]
pub struct RuntimeAssignmentState {
    pub assignment_uuid: String,
    pub target_uuid: String,
    pub tenant_id: i64,
    pub node_uuid: String,
    pub environment: RuntimeEnvironment,
    pub trigger_app_revision_id: Option<i64>,
    pub generation: u64,
    pub snapshot_uuid: String,
    pub snapshot_sha256: String,
    pub desired_state_sha256: String,
    pub runtime_set: Value,
    pub publish_status: RuntimeAssignmentPublishStatus,
    pub remote_assignment_uuid: Option<String>,
    pub attempt_count: i32,
    pub lease_owner: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuntimeAssignmentPublishStatus {
    Pending,
    Publishing,
    Published,
    Failed,
    Superseded,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RuntimePublicationBatchResult {
    pub claimed: usize,
    pub published: usize,
    pub failed: usize,
    pub observations_checked: usize,
    pub observations_ingested: usize,
    pub observations_pending: usize,
    pub observations_failed: usize,
    pub revisions_activated: usize,
    pub provider_event_assignments_checked: usize,
    pub provider_event_assignments_failed: usize,
    pub provider_event_deliveries_ensured: usize,
    pub provider_event_deliveries_skipped: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuntimeObservationState {
    Received,
    Validated,
    Staged,
    Active,
    Rejected,
}

impl RuntimeObservationState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Received => "RECEIVED",
            Self::Validated => "VALIDATED",
            Self::Staged => "STAGED",
            Self::Active => "ACTIVE",
            Self::Rejected => "REJECTED",
        }
    }

    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Active | Self::Rejected)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeObservationEvidence {
    pub observation_uuid: String,
    pub assignment_uuid: String,
    pub tenant_id: i64,
    pub node_uuid: String,
    pub environment: RuntimeEnvironment,
    pub generation: u64,
    pub snapshot_uuid: String,
    pub snapshot_sha256: String,
    pub state: RuntimeObservationState,
    pub node_version: Option<String>,
    pub reason_code: Option<String>,
    pub detail: Option<String>,
    pub observed_at: String,
}

impl RuntimeObservationEvidence {
    pub fn validate_for(&self, assignment: &RuntimeAssignmentState) -> DeployServiceResult<()> {
        let remote_assignment_uuid = assignment
            .remote_assignment_uuid
            .as_deref()
            .ok_or_else(|| DeployServiceError::conflict("runtime assignment is not published"))?;
        if assignment.publish_status != RuntimeAssignmentPublishStatus::Published
            || self.assignment_uuid != remote_assignment_uuid
            || self.tenant_id != assignment.tenant_id
            || self.node_uuid != assignment.node_uuid
            || self.environment != assignment.environment
            || self.generation != assignment.generation
            || self.snapshot_uuid != assignment.snapshot_uuid
            || self.snapshot_sha256 != assignment.snapshot_sha256
        {
            return Err(DeployServiceError::conflict(
                "Web runtime observation identity does not match the durable assignment",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RuntimeObservationPersistenceResult {
    pub inserted: bool,
    pub revision_activated: bool,
}

#[derive(Clone, Debug)]
pub struct PersistRuntimeAssignmentCommand {
    pub assignment_uuid: String,
    pub snapshot_uuid: String,
    pub snapshot_sha256: String,
    pub desired_state_sha256: String,
    pub runtime_set: Value,
    pub runtime_set_bytes: usize,
    pub created_at: String,
}

#[async_trait]
pub trait DeployRuntimeAssignmentMutationPort: Send {
    fn latest_runtime_assignment(&self) -> Option<&RuntimeAssignmentState>;

    fn next_generation(&self) -> u64;

    async fn commit_without_change(self: Box<Self>) -> DeployServiceResult<()>;

    async fn persist_runtime_assignment(
        self: Box<Self>,
        command: PersistRuntimeAssignmentCommand,
    ) -> DeployServiceResult<RuntimeAssignmentState>;
}

#[async_trait]
pub trait DeployRuntimeAssignmentRepositoryPort: Send + Sync {
    async fn latest_runtime_assignment(
        &self,
        target_uuid: &str,
    ) -> DeployServiceResult<Option<RuntimeAssignmentState>>;

    async fn begin_runtime_assignment_mutation(
        &self,
        target_uuid: &str,
        tenant_id: i64,
    ) -> DeployServiceResult<Box<dyn DeployRuntimeAssignmentMutationPort>>;

    async fn claim_due_runtime_assignments(
        &self,
        maximum_items: i64,
        now: &str,
        lease_owner: &str,
        lease_expires_at: &str,
        maximum_attempts: i32,
    ) -> DeployServiceResult<Vec<RuntimeAssignmentState>>;

    async fn mark_runtime_assignment_published(
        &self,
        assignment_uuid: &str,
        lease_owner: &str,
        receipt: &RuntimeAssignmentReceipt,
        published_at: &str,
    ) -> DeployServiceResult<()>;

    async fn mark_runtime_assignment_failed(
        &self,
        assignment_uuid: &str,
        lease_owner: &str,
        error_code: &str,
        next_attempt_at: Option<&str>,
        updated_at: &str,
    ) -> DeployServiceResult<()>;

    async fn list_runtime_assignments_requiring_observation(
        &self,
        maximum_items: i64,
    ) -> DeployServiceResult<Vec<RuntimeAssignmentState>>;

    async fn list_active_runtime_assignments_after(
        &self,
        after_target_uuid: Option<&str>,
        maximum_items: i64,
    ) -> DeployServiceResult<Vec<RuntimeAssignmentState>>;

    async fn persist_runtime_observation(
        &self,
        assignment_uuid: &str,
        observation: &RuntimeObservationEvidence,
        ingested_at: &str,
    ) -> DeployServiceResult<RuntimeObservationPersistenceResult>;
}

pub struct RuntimePublicationService {
    repository: Arc<dyn DeployRuntimeAssignmentRepositoryPort>,
    web_runtime: Arc<dyn DeployWebRuntimePort>,
    provider_event_delivery: Arc<dyn WebsiteProviderEventDeliveryPort>,
    provider_event_renewal_cursor: Mutex<Option<String>>,
    maximum_sites: usize,
}

impl RuntimePublicationService {
    pub fn new(
        repository: Arc<dyn DeployRuntimeAssignmentRepositoryPort>,
        web_runtime: Arc<dyn DeployWebRuntimePort>,
    ) -> Self {
        Self {
            repository,
            web_runtime,
            provider_event_delivery: Arc::new(NoopWebsiteProviderEventDeliveryPort),
            provider_event_renewal_cursor: Mutex::new(None),
            maximum_sites: DEFAULT_MAXIMUM_SITES,
        }
    }

    pub fn new_with_provider_event_delivery(
        repository: Arc<dyn DeployRuntimeAssignmentRepositoryPort>,
        web_runtime: Arc<dyn DeployWebRuntimePort>,
        provider_event_delivery: Arc<dyn WebsiteProviderEventDeliveryPort>,
    ) -> Self {
        Self {
            repository,
            web_runtime,
            provider_event_delivery,
            provider_event_renewal_cursor: Mutex::new(None),
            maximum_sites: DEFAULT_MAXIMUM_SITES,
        }
    }

    pub fn with_maximum_sites(mut self, maximum_sites: usize) -> DeployServiceResult<Self> {
        if maximum_sites == 0 || maximum_sites > DEFAULT_MAXIMUM_SITES {
            return Err(DeployServiceError::validation(
                "maximumSites is outside the supported range",
            ));
        }
        self.maximum_sites = maximum_sites;
        Ok(self)
    }

    pub async fn enqueue_target_state(
        &self,
        target: &RuntimeTarget,
        snapshot_uuid: String,
        generated_at: String,
        mut descriptors: Vec<Value>,
    ) -> DeployServiceResult<RuntimeAssignmentState> {
        validate_target_scope(target, &descriptors)?;
        normalize_runtime_descriptors(&mut descriptors);
        let desired_state_sha256 = desired_state_sha256(&descriptors)?;
        let mutation = self
            .repository
            .begin_runtime_assignment_mutation(&target.target_uuid, target.tenant_id)
            .await?;
        if let Some(latest) = mutation
            .latest_runtime_assignment()
            .filter(|assignment| {
                assignment.desired_state_sha256 == desired_state_sha256
                    && assignment.publish_status != RuntimeAssignmentPublishStatus::Superseded
            })
            .cloned()
        {
            mutation.commit_without_change().await?;
            return Ok(latest);
        }
        let generation = mutation.next_generation();
        if generation > 9_007_199_254_740_991 {
            return Err(DeployServiceError::conflict(
                "runtime assignment generation is exhausted",
            ));
        }
        let compiled = compile_runtime_set(RuntimeSetCompilationInput {
            snapshot_uuid: snapshot_uuid.clone(),
            node_uuid: target.node_uuid.clone(),
            environment: target.environment,
            generation,
            generated_at: generated_at.clone(),
            maximum_sites: self.maximum_sites,
            descriptors,
        })
        .map_err(|error| DeployServiceError::validation(error.to_string()))?;
        let runtime_set_bytes = runtime_set_size_bytes(&compiled.snapshot)
            .map_err(|error| DeployServiceError::validation(error.to_string()))?;
        mutation
            .persist_runtime_assignment(PersistRuntimeAssignmentCommand {
                assignment_uuid: sdkwork_database_id::uuid_v4(),
                snapshot_uuid,
                snapshot_sha256: compiled.snapshot_sha256,
                desired_state_sha256,
                runtime_set: compiled.snapshot,
                runtime_set_bytes,
                created_at: generated_at,
            })
            .await
    }

    pub async fn publish_assignment(
        &self,
        assignment: &RuntimeAssignmentState,
    ) -> DeployServiceResult<()> {
        self.publish_assignment_with_provider_events(assignment)
            .await
            .map(|_| ())
    }

    async fn publish_assignment_with_provider_events(
        &self,
        assignment: &RuntimeAssignmentState,
    ) -> DeployServiceResult<WebsiteProviderEventDeliveryResult> {
        if assignment.publish_status != RuntimeAssignmentPublishStatus::Publishing {
            return Err(DeployServiceError::conflict(
                "runtime assignment must hold a publication lease",
            ));
        }
        let lease_owner = assignment.lease_owner.as_deref().ok_or_else(|| {
            DeployServiceError::conflict("runtime assignment publication lease owner is missing")
        })?;
        let compiled = CompiledRuntimeSet {
            snapshot: assignment.runtime_set.clone(),
            snapshot_sha256: assignment.snapshot_sha256.clone(),
        };
        let provider_events = match self
            .provider_event_delivery
            .ensure_runtime_set(&assignment.node_uuid, &compiled.snapshot)
            .await
        {
            Ok(result) => result,
            Err(error) => {
                let next_attempt = next_attempt_at(assignment.attempt_count);
                let now = now_seconds();
                self.repository
                    .mark_runtime_assignment_failed(
                        &assignment.assignment_uuid,
                        lease_owner,
                        "PROVIDER_EVENT_DELIVERY_UNAVAILABLE",
                        next_attempt.as_deref(),
                        &now,
                    )
                    .await?;
                return Err(error);
            }
        };
        match self.web_runtime.publish_runtime_assignment(&compiled).await {
            Ok(receipt) => {
                if let Err(error) = validate_receipt(assignment, &receipt) {
                    let next_attempt = next_attempt_at(assignment.attempt_count);
                    let now = now_seconds();
                    self.repository
                        .mark_runtime_assignment_failed(
                            &assignment.assignment_uuid,
                            lease_owner,
                            "WEB_ASSIGNMENT_RECEIPT_MISMATCH",
                            next_attempt.as_deref(),
                            &now,
                        )
                        .await?;
                    return Err(error);
                }
                let now = now_seconds();
                self.repository
                    .mark_runtime_assignment_published(
                        &assignment.assignment_uuid,
                        lease_owner,
                        &receipt,
                        &now,
                    )
                    .await?;
                Ok(provider_events)
            }
            Err(error) => {
                let next_attempt = next_attempt_at(assignment.attempt_count);
                let error_code = publication_error_code(&error);
                let now = now_seconds();
                self.repository
                    .mark_runtime_assignment_failed(
                        &assignment.assignment_uuid,
                        lease_owner,
                        error_code,
                        next_attempt.as_deref(),
                        &now,
                    )
                    .await?;
                Err(error)
            }
        }
    }

    pub async fn publish_due(
        &self,
        worker_id: &str,
        maximum_items: i64,
        lease_seconds: i64,
    ) -> DeployServiceResult<RuntimePublicationBatchResult> {
        if !(1..=MAXIMUM_PUBLICATION_BATCH).contains(&maximum_items) {
            return Err(DeployServiceError::validation(
                "runtime publication batch must be between 1 and 100",
            ));
        }
        validate_worker_id(worker_id)?;
        if !(MINIMUM_LEASE_SECONDS..=MAXIMUM_LEASE_SECONDS).contains(&lease_seconds) {
            return Err(DeployServiceError::validation(
                "runtime publication lease must be between 5 and 300 seconds",
            ));
        }
        let now = Utc::now();
        let now_text = now.to_rfc3339_opts(SecondsFormat::Secs, true);
        let lease_expires_at =
            (now + Duration::seconds(lease_seconds)).to_rfc3339_opts(SecondsFormat::Secs, true);
        let assignments = self
            .repository
            .claim_due_runtime_assignments(
                maximum_items,
                &now_text,
                worker_id,
                &lease_expires_at,
                MAXIMUM_ATTEMPTS,
            )
            .await?;
        let mut result = RuntimePublicationBatchResult {
            claimed: assignments.len(),
            ..RuntimePublicationBatchResult::default()
        };
        for assignment in assignments {
            match self
                .publish_assignment_with_provider_events(&assignment)
                .await
            {
                Ok(provider_events) => {
                    result.published += 1;
                    result.provider_event_deliveries_ensured += provider_events.ensured;
                    result.provider_event_deliveries_skipped += provider_events.skipped;
                }
                Err(_) => result.failed += 1,
            }
        }
        let observation_result = self.reconcile_observations(maximum_items).await?;
        result.observations_checked = observation_result.observations_checked;
        result.observations_ingested = observation_result.observations_ingested;
        result.observations_pending = observation_result.observations_pending;
        result.observations_failed = observation_result.observations_failed;
        result.revisions_activated = observation_result.revisions_activated;
        let provider_event_result = self
            .renew_active_provider_event_deliveries(maximum_items)
            .await?;
        result.provider_event_assignments_checked =
            provider_event_result.provider_event_assignments_checked;
        result.provider_event_assignments_failed =
            provider_event_result.provider_event_assignments_failed;
        result.provider_event_deliveries_ensured +=
            provider_event_result.provider_event_deliveries_ensured;
        result.provider_event_deliveries_skipped +=
            provider_event_result.provider_event_deliveries_skipped;
        Ok(result)
    }

    pub async fn renew_active_provider_event_deliveries(
        &self,
        maximum_items: i64,
    ) -> DeployServiceResult<RuntimePublicationBatchResult> {
        if !(1..=MAXIMUM_PUBLICATION_BATCH).contains(&maximum_items) {
            return Err(DeployServiceError::validation(
                "provider event renewal batch must be between 1 and 100",
            ));
        }
        let cursor = self
            .provider_event_renewal_cursor
            .lock()
            .map_err(|_| {
                DeployServiceError::Internal(
                    "provider event renewal cursor is unavailable".to_owned(),
                )
            })?
            .clone();
        let assignments = self
            .repository
            .list_active_runtime_assignments_after(cursor.as_deref(), maximum_items)
            .await?;
        let mut result = RuntimePublicationBatchResult {
            provider_event_assignments_checked: assignments.len(),
            ..RuntimePublicationBatchResult::default()
        };
        for assignment in &assignments {
            match self
                .provider_event_delivery
                .ensure_runtime_set(&assignment.node_uuid, &assignment.runtime_set)
                .await
            {
                Ok(deliveries) => {
                    result.provider_event_deliveries_ensured += deliveries.ensured;
                    result.provider_event_deliveries_skipped += deliveries.skipped;
                }
                Err(error) => {
                    tracing::warn!(
                        target_uuid = %assignment.target_uuid,
                        node_uuid = %assignment.node_uuid,
                        error_kind = ?error.kind(),
                        "website provider event delivery renewal failed"
                    );
                    result.provider_event_assignments_failed += 1;
                }
            }
        }
        let next_cursor = if assignments.len() == maximum_items as usize {
            assignments
                .last()
                .map(|assignment| assignment.target_uuid.clone())
        } else {
            None
        };
        *self.provider_event_renewal_cursor.lock().map_err(|_| {
            DeployServiceError::Internal("provider event renewal cursor is unavailable".to_owned())
        })? = next_cursor;
        Ok(result)
    }

    pub async fn reconcile_observations(
        &self,
        maximum_items: i64,
    ) -> DeployServiceResult<RuntimePublicationBatchResult> {
        if !(1..=MAXIMUM_PUBLICATION_BATCH).contains(&maximum_items) {
            return Err(DeployServiceError::validation(
                "runtime observation batch must be between 1 and 100",
            ));
        }
        let assignments = self
            .repository
            .list_runtime_assignments_requiring_observation(maximum_items)
            .await?;
        let mut result = RuntimePublicationBatchResult {
            observations_checked: assignments.len(),
            ..RuntimePublicationBatchResult::default()
        };
        for assignment in assignments {
            match self
                .web_runtime
                .retrieve_latest_runtime_observation(&assignment.snapshot_uuid)
                .await
            {
                Ok(receipt) => match validate_observation_receipt(&assignment, receipt) {
                    Ok(observation) => {
                        let persistence = self
                            .repository
                            .persist_runtime_observation(
                                &assignment.assignment_uuid,
                                &observation,
                                &now_seconds(),
                            )
                            .await;
                        match persistence {
                            Ok(persistence) => {
                                result.observations_ingested += usize::from(persistence.inserted);
                                result.revisions_activated +=
                                    usize::from(persistence.revision_activated);
                            }
                            Err(error) => {
                                tracing::warn!(
                                    assignment_uuid = %assignment.assignment_uuid,
                                    error_kind = ?error.kind(),
                                    "runtime observation persistence failed"
                                );
                                result.observations_failed += 1;
                            }
                        }
                    }
                    Err(error) => {
                        tracing::warn!(
                            assignment_uuid = %assignment.assignment_uuid,
                            error_kind = ?error.kind(),
                            "runtime observation identity validation failed"
                        );
                        result.observations_failed += 1;
                    }
                },
                Err(error) if error.kind() == DeployServiceErrorKind::NotFound => {
                    result.observations_pending += 1;
                }
                Err(error) => {
                    tracing::warn!(
                        assignment_uuid = %assignment.assignment_uuid,
                        error_kind = ?error.kind(),
                        "runtime observation retrieval failed"
                    );
                    result.observations_failed += 1;
                }
            }
        }
        Ok(result)
    }
}

fn validate_observation_receipt(
    assignment: &RuntimeAssignmentState,
    receipt: RuntimeObservationReceipt,
) -> DeployServiceResult<RuntimeObservationEvidence> {
    validate_opaque_id(&receipt.observation_uuid, 128, "observationUuid")?;
    validate_opaque_id(&receipt.assignment_uuid, 128, "assignmentUuid")?;
    validate_opaque_id(&receipt.node_uuid, 128, "nodeUuid")?;
    validate_opaque_id(&receipt.snapshot_uuid, 128, "snapshotUuid")?;
    validate_sha256(&receipt.snapshot_sha256, "snapshotSha256")?;
    validate_optional_text(receipt.node_version.as_deref(), 64, "nodeVersion")?;
    validate_optional_text(receipt.reason_code.as_deref(), 64, "reasonCode")?;
    validate_optional_text(receipt.detail.as_deref(), 512, "detail")?;
    DateTime::parse_from_rfc3339(&receipt.observed_at).map_err(|_| {
        DeployServiceError::validation("Web runtime observation observedAt is invalid")
    })?;
    let tenant_id = parse_positive_i64(&receipt.tenant_id, "tenantId")?;
    let generation = parse_generation(&receipt.generation)?;
    let environment = parse_runtime_environment(&receipt.environment)?;
    let state = parse_observation_state(&receipt.state)?;
    validate_observation_reason(
        state,
        receipt.reason_code.as_deref(),
        receipt.detail.as_deref(),
    )?;
    let observation = RuntimeObservationEvidence {
        observation_uuid: receipt.observation_uuid,
        assignment_uuid: receipt.assignment_uuid,
        tenant_id,
        node_uuid: receipt.node_uuid,
        environment,
        generation,
        snapshot_uuid: receipt.snapshot_uuid,
        snapshot_sha256: receipt.snapshot_sha256,
        state,
        node_version: receipt.node_version,
        reason_code: receipt.reason_code,
        detail: receipt.detail,
        observed_at: receipt.observed_at,
    };
    observation.validate_for(assignment)?;
    Ok(observation)
}

fn validate_worker_id(worker_id: &str) -> DeployServiceResult<()> {
    if worker_id.is_empty()
        || worker_id.len() > 128
        || !worker_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'-'))
    {
        return Err(DeployServiceError::validation(
            "runtime publication workerId is invalid",
        ));
    }
    Ok(())
}

fn parse_positive_i64(value: &str, field: &str) -> DeployServiceResult<i64> {
    if value.is_empty()
        || value.len() > 19
        || value.starts_with('0')
        || !value.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(DeployServiceError::validation(format!(
            "Web runtime observation {field} is invalid"
        )));
    }
    value.parse::<i64>().map_err(|_| {
        DeployServiceError::validation(format!(
            "Web runtime observation {field} is outside the supported range"
        ))
    })
}

fn parse_generation(value: &str) -> DeployServiceResult<u64> {
    if value.is_empty()
        || value.len() > 16
        || value.starts_with('0')
        || !value.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(DeployServiceError::validation(
            "Web runtime observation generation is invalid",
        ));
    }
    let generation = value.parse::<u64>().map_err(|_| {
        DeployServiceError::validation(
            "Web runtime observation generation is outside the supported range",
        )
    })?;
    if generation == 0 || generation > 9_007_199_254_740_991 {
        return Err(DeployServiceError::validation(
            "Web runtime observation generation is outside the supported range",
        ));
    }
    Ok(generation)
}

fn parse_runtime_environment(value: &str) -> DeployServiceResult<RuntimeEnvironment> {
    RuntimeEnvironment::parse(value).map_err(|_| {
        DeployServiceError::validation("Web runtime observation environment is invalid")
    })
}

fn parse_observation_state(value: &str) -> DeployServiceResult<RuntimeObservationState> {
    match value {
        "RECEIVED" => Ok(RuntimeObservationState::Received),
        "VALIDATED" => Ok(RuntimeObservationState::Validated),
        "STAGED" => Ok(RuntimeObservationState::Staged),
        "ACTIVE" => Ok(RuntimeObservationState::Active),
        "REJECTED" => Ok(RuntimeObservationState::Rejected),
        _ => Err(DeployServiceError::validation(
            "Web runtime observation state is invalid",
        )),
    }
}

fn validate_sha256(value: &str, field: &str) -> DeployServiceResult<()> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(DeployServiceError::validation(format!(
            "Web runtime observation {field} is invalid"
        )));
    }
    Ok(())
}

fn validate_opaque_id(value: &str, maximum: usize, field: &str) -> DeployServiceResult<()> {
    if value.is_empty()
        || value.len() > maximum
        || !value.bytes().enumerate().all(|(index, byte)| {
            byte.is_ascii_alphanumeric() || (index > 0 && matches!(byte, b'.' | b'_' | b':' | b'-'))
        })
    {
        return Err(DeployServiceError::validation(format!(
            "Web runtime observation {field} is invalid"
        )));
    }
    Ok(())
}

fn validate_optional_text(
    value: Option<&str>,
    maximum: usize,
    field: &str,
) -> DeployServiceResult<()> {
    if value.is_some_and(|value| {
        value.is_empty()
            || value.len() > maximum
            || value.bytes().any(|byte| byte.is_ascii_control())
    }) {
        return Err(DeployServiceError::validation(format!(
            "Web runtime observation {field} is invalid"
        )));
    }
    Ok(())
}

fn validate_observation_reason(
    state: RuntimeObservationState,
    reason_code: Option<&str>,
    detail: Option<&str>,
) -> DeployServiceResult<()> {
    if reason_code.is_some_and(|value| {
        !value.bytes().enumerate().all(|(index, byte)| {
            (index == 0 && byte.is_ascii_uppercase())
                || (index > 0
                    && (byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_'))
        })
    }) {
        return Err(DeployServiceError::validation(
            "Web runtime observation reasonCode is invalid",
        ));
    }
    match state {
        RuntimeObservationState::Rejected if reason_code.is_none() => Err(
            DeployServiceError::validation("REJECTED runtime observations require reasonCode"),
        ),
        RuntimeObservationState::Rejected => Ok(()),
        _ if reason_code.is_some() || detail.is_some() => Err(DeployServiceError::validation(
            "Only REJECTED runtime observations may include reason details",
        )),
        _ => Ok(()),
    }
}

fn validate_target_scope(target: &RuntimeTarget, descriptors: &[Value]) -> DeployServiceResult<()> {
    if target.tenant_id <= 0 {
        return Err(DeployServiceError::forbidden(
            "assignment publication forbidden",
        ));
    }
    if target.tenant_scope_hash.len() != 64
        || !target
            .tenant_scope_hash
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(DeployServiceError::validation(
            "runtime target tenant scope hash is invalid",
        ));
    }
    for descriptor in descriptors {
        let scope = descriptor
            .get("tenantScopeHash")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                DeployServiceError::validation("descriptor tenantScopeHash is required")
            })?;
        if scope != target.tenant_scope_hash {
            return Err(DeployServiceError::forbidden(
                "assignment publication forbidden",
            ));
        }
    }
    Ok(())
}

fn desired_state_sha256(descriptors: &[Value]) -> DeployServiceResult<String> {
    canonical_sha256_excluding_field(
        &serde_json::json!({"descriptors": descriptors}),
        "__no_excluded_field",
    )
    .map_err(|error| DeployServiceError::Internal(error.to_string()))
}

fn validate_receipt(
    assignment: &RuntimeAssignmentState,
    receipt: &RuntimeAssignmentReceipt,
) -> DeployServiceResult<()> {
    if receipt.assignment_uuid.is_empty()
        || receipt.assignment_uuid.len() > 128
        || receipt.node_uuid != assignment.node_uuid
        || receipt.environment != assignment.environment.as_str()
        || receipt.snapshot_uuid != assignment.snapshot_uuid
        || receipt.snapshot_sha256 != assignment.snapshot_sha256
        || receipt.generation != assignment.generation.to_string()
    {
        return Err(DeployServiceError::conflict(
            "Web runtime assignment receipt does not match the durable assignment",
        ));
    }
    Ok(())
}

fn publication_error_code(error: &DeployServiceError) -> &'static str {
    match error.kind() {
        DeployServiceErrorKind::NotFound => "WEB_TARGET_NOT_FOUND",
        DeployServiceErrorKind::Conflict => "WEB_ASSIGNMENT_CONFLICT",
        DeployServiceErrorKind::Validation => "WEB_ASSIGNMENT_REJECTED",
        DeployServiceErrorKind::Forbidden => "WEB_ASSIGNMENT_FORBIDDEN",
        DeployServiceErrorKind::QuotaExceeded => "WEB_ASSIGNMENT_QUOTA_EXCEEDED",
        DeployServiceErrorKind::DatabaseUnavailable => "WEB_DATABASE_UNAVAILABLE",
        DeployServiceErrorKind::Internal => "WEB_PUBLICATION_UNAVAILABLE",
    }
}

fn next_attempt_at(attempt_count: i32) -> Option<String> {
    if attempt_count >= MAXIMUM_ATTEMPTS {
        return None;
    }
    let exponent = attempt_count.clamp(1, 10) as u32;
    let seconds = 1_i64 << exponent;
    Some((Utc::now() + Duration::seconds(seconds)).to_rfc3339_opts(SecondsFormat::Secs, true))
}

fn now_seconds() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_scope_rejects_cross_tenant_descriptor() {
        let target = RuntimeTarget {
            target_uuid: "target-1".to_owned(),
            tenant_id: 1,
            node_uuid: "node-1".to_owned(),
            environment: RuntimeEnvironment::Production,
            tenant_scope_hash: "1".repeat(64),
        };
        let descriptor = serde_json::json!({"tenantScopeHash": "2".repeat(64)});
        assert!(matches!(
            validate_target_scope(&target, &[descriptor]),
            Err(DeployServiceError::Forbidden(_))
        ));
    }

    #[test]
    fn retry_schedule_is_bounded() {
        assert!(next_attempt_at(1).is_some());
        assert!(next_attempt_at(MAXIMUM_ATTEMPTS).is_none());
    }
}
