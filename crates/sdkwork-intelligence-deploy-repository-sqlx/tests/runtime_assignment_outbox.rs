mod common;

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use sdkwork_database_id::SnowflakeIdGenerator;
use sdkwork_deploy_content_provider_port::{
    WebsiteProviderEventDeliveryPort, WebsiteProviderEventDeliveryResult,
};
use sdkwork_deploy_contract::{DeployServiceError, DeployServiceResult};
use sdkwork_deploy_runtime_compiler::{
    canonical_sha256_excluding_field, CompiledRuntimeSet, RuntimeEnvironment,
};
use sdkwork_deploy_web_port::{
    DeployWebRuntimePort, RuntimeAssignmentReceipt, RuntimeObservationReceipt,
};
use sdkwork_intelligence_deploy_repository_sqlx::DeployRepository;
use sdkwork_intelligence_deploy_service::runtime_publication::{
    DeployRuntimeAssignmentRepositoryPort, RuntimeAssignmentPublishStatus, RuntimeTarget,
};
use sdkwork_intelligence_deploy_service::RuntimePublicationService;
use sqlx::Row;

#[derive(Default)]
struct AcceptingWebRuntime;

struct SelectiveActiveWebRuntime {
    tenant_id: i64,
    published: Mutex<HashMap<String, RuntimeAssignmentReceipt>>,
    active_snapshots: Mutex<HashSet<String>>,
}

#[derive(Default)]
struct CountingWebRuntime {
    calls: AtomicUsize,
}

#[derive(Default)]
struct FailingProviderEventDelivery;

struct SelectiveProviderEventDelivery {
    visited_nodes: Mutex<Vec<String>>,
    failing_node: Option<String>,
}

impl SelectiveProviderEventDelivery {
    fn new(failing_node: Option<&str>) -> Self {
        Self {
            visited_nodes: Mutex::new(Vec::new()),
            failing_node: failing_node.map(str::to_owned),
        }
    }
}

impl SelectiveActiveWebRuntime {
    fn new(tenant_id: i64, active_snapshots: impl IntoIterator<Item = String>) -> Self {
        Self {
            tenant_id,
            published: Mutex::new(HashMap::new()),
            active_snapshots: Mutex::new(active_snapshots.into_iter().collect()),
        }
    }

    fn activate(&self, snapshot_uuid: &str) {
        self.active_snapshots
            .lock()
            .unwrap()
            .insert(snapshot_uuid.to_owned());
    }
}

fn runtime_descriptor(app_uuid: &str, tenant_scope_hash: &str) -> serde_json::Value {
    let mut descriptor = serde_json::json!({
        "appUuid": app_uuid,
        "environment": "production",
        "tenantScopeHash": tenant_scope_hash,
        "descriptorSha256": ""
    });
    let descriptor_sha256 = canonical_sha256_excluding_field(&descriptor, "descriptorSha256")
        .expect("hash runtime descriptor");
    descriptor["descriptorSha256"] = serde_json::Value::String(descriptor_sha256);
    descriptor
}

#[async_trait]
impl DeployWebRuntimePort for AcceptingWebRuntime {
    async fn publish_runtime_assignment(
        &self,
        runtime_set: &CompiledRuntimeSet,
    ) -> DeployServiceResult<RuntimeAssignmentReceipt> {
        Ok(RuntimeAssignmentReceipt {
            assignment_uuid: "web-assignment-1".to_owned(),
            node_uuid: runtime_set.snapshot["nodeUuid"]
                .as_str()
                .unwrap()
                .to_owned(),
            environment: runtime_set.snapshot["environment"]
                .as_str()
                .unwrap()
                .to_owned(),
            generation: runtime_set.snapshot["generation"].to_string(),
            snapshot_uuid: runtime_set.snapshot["snapshotUuid"]
                .as_str()
                .unwrap()
                .to_owned(),
            snapshot_sha256: runtime_set.snapshot_sha256.clone(),
            assigned_at: "2026-07-22T00:00:01Z".to_owned(),
        })
    }

    async fn retrieve_latest_runtime_observation(
        &self,
        _snapshot_uuid: &str,
    ) -> DeployServiceResult<RuntimeObservationReceipt> {
        Err(DeployServiceError::not_found(
            "runtime observation not available",
        ))
    }
}

#[async_trait]
impl DeployWebRuntimePort for CountingWebRuntime {
    async fn publish_runtime_assignment(
        &self,
        _runtime_set: &CompiledRuntimeSet,
    ) -> DeployServiceResult<RuntimeAssignmentReceipt> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Err(DeployServiceError::Internal(
            "Web runtime must not be reached".to_owned(),
        ))
    }

    async fn retrieve_latest_runtime_observation(
        &self,
        _snapshot_uuid: &str,
    ) -> DeployServiceResult<RuntimeObservationReceipt> {
        Err(DeployServiceError::not_found(
            "runtime observation not available",
        ))
    }
}

#[async_trait]
impl WebsiteProviderEventDeliveryPort for FailingProviderEventDelivery {
    async fn ensure_runtime_set(
        &self,
        _node_uuid: &str,
        _runtime_set: &serde_json::Value,
    ) -> DeployServiceResult<WebsiteProviderEventDeliveryResult> {
        Err(DeployServiceError::Internal(
            "provider event delivery is unavailable".to_owned(),
        ))
    }
}

#[async_trait]
impl WebsiteProviderEventDeliveryPort for SelectiveProviderEventDelivery {
    async fn ensure_runtime_set(
        &self,
        node_uuid: &str,
        _runtime_set: &serde_json::Value,
    ) -> DeployServiceResult<WebsiteProviderEventDeliveryResult> {
        self.visited_nodes
            .lock()
            .unwrap()
            .push(node_uuid.to_owned());
        if self.failing_node.as_deref() == Some(node_uuid) {
            return Err(DeployServiceError::Internal(
                "selected provider event delivery failure".to_owned(),
            ));
        }
        Ok(WebsiteProviderEventDeliveryResult {
            ensured: 1,
            skipped: 0,
        })
    }
}

#[async_trait]
impl DeployWebRuntimePort for SelectiveActiveWebRuntime {
    async fn publish_runtime_assignment(
        &self,
        runtime_set: &CompiledRuntimeSet,
    ) -> DeployServiceResult<RuntimeAssignmentReceipt> {
        let snapshot_uuid = runtime_set.snapshot["snapshotUuid"]
            .as_str()
            .unwrap()
            .to_owned();
        let receipt = RuntimeAssignmentReceipt {
            assignment_uuid: format!("web-{snapshot_uuid}"),
            node_uuid: runtime_set.snapshot["nodeUuid"]
                .as_str()
                .unwrap()
                .to_owned(),
            environment: runtime_set.snapshot["environment"]
                .as_str()
                .unwrap()
                .to_owned(),
            generation: runtime_set.snapshot["generation"].to_string(),
            snapshot_uuid: snapshot_uuid.clone(),
            snapshot_sha256: runtime_set.snapshot_sha256.clone(),
            assigned_at: "2026-07-22T00:00:01Z".to_owned(),
        };
        self.published
            .lock()
            .unwrap()
            .insert(snapshot_uuid, receipt.clone());
        Ok(receipt)
    }

    async fn retrieve_latest_runtime_observation(
        &self,
        snapshot_uuid: &str,
    ) -> DeployServiceResult<RuntimeObservationReceipt> {
        if !self
            .active_snapshots
            .lock()
            .unwrap()
            .contains(snapshot_uuid)
        {
            return Err(DeployServiceError::not_found(
                "runtime observation not available",
            ));
        }
        let assignment = self
            .published
            .lock()
            .unwrap()
            .get(snapshot_uuid)
            .cloned()
            .ok_or_else(|| DeployServiceError::not_found("runtime assignment not published"))?;
        Ok(RuntimeObservationReceipt {
            observation_uuid: format!("observation-{snapshot_uuid}"),
            assignment_uuid: assignment.assignment_uuid,
            tenant_id: self.tenant_id.to_string(),
            node_uuid: assignment.node_uuid,
            environment: assignment.environment,
            generation: assignment.generation,
            snapshot_uuid: assignment.snapshot_uuid,
            snapshot_sha256: assignment.snapshot_sha256,
            state: "ACTIVE".to_owned(),
            node_version: Some("1.0.0".to_owned()),
            reason_code: None,
            detail: None,
            observed_at: "2026-07-22T00:00:02Z".to_owned(),
        })
    }
}

#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn site_revision_activates_only_after_all_frozen_targets_are_active() {
    let pool = common::postgres_pool().await;
    sqlx::query(
        "INSERT INTO deploy_app (
            id, uuid, tenant_id, name, slug, app_kind, app_status, runtime_config, metadata,
            default_environment, created_at, updated_at, version
         ) VALUES (100, 'site-100', 7, 'Site 100', 'site-100', 'WEB', 'ACTIVE', '{}', '{}',
                   'production', '2026-07-22T00:00:00Z', '2026-07-22T00:00:00Z', 1)",
    )
    .execute(&pool)
    .await
    .expect("insert site");
    sqlx::query(
        "INSERT INTO deploy_app_revision (
            id, uuid, tenant_id, app_id, revision_no, environment,
            descriptor_schema_version, descriptor_json, descriptor_sha256,
            compiler_version, source_config_version, idempotency_key, request_sha256,
            result_json, validation_status, validation_report_json, created_at
         ) VALUES (
            200, 'revision-200', 7, 100, 1, 'production',
            'sdkwork.website-runtime.v1', '{}', $1, 'test/1', 1,
            'revision-200', $2, '{}', 'VALID', '{}', '2026-07-22T00:00:00Z'
         )",
    )
    .bind("a".repeat(64))
    .bind("b".repeat(64))
    .execute(&pool)
    .await
    .expect("insert site revision");
    sqlx::query("UPDATE deploy_app SET desired_revision_id = 200 WHERE id = 100")
        .execute(&pool)
        .await
        .expect("set desired revision");
    for (id, target_uuid, node_uuid) in
        [(1_i64, "target-a", "node-a"), (2_i64, "target-b", "node-b")]
    {
        sqlx::query(
            "INSERT INTO deploy_web_node_target (
                id, uuid, tenant_id, node_uuid, environment, tenant_scope_hash,
                status, created_at, updated_at, version
             ) VALUES ($1, $2, 7, $3, 'production', $4,
                       'ACTIVE', '2026-07-22T00:00:00Z', '2026-07-22T00:00:00Z', 1)",
        )
        .bind(id)
        .bind(target_uuid)
        .bind(node_uuid)
        .bind("1".repeat(64))
        .execute(&pool)
        .await
        .expect("insert Web Node target");
    }

    let repository = Arc::new(DeployRepository::new(
        pool.clone(),
        SnowflakeIdGenerator::new(1).expect("Snowflake generator"),
        common::test_secret_key(),
    ));
    let web_runtime = Arc::new(SelectiveActiveWebRuntime::new(7, ["snapshot-a".to_owned()]));
    let publisher = RuntimePublicationService::new(repository, web_runtime.clone());
    for (target_uuid, node_uuid, snapshot_uuid) in [
        ("target-a", "node-a", "snapshot-a"),
        ("target-b", "node-b", "snapshot-b"),
    ] {
        publisher
            .enqueue_target_state(
                &RuntimeTarget {
                    target_uuid: target_uuid.to_owned(),
                    tenant_id: 7,
                    node_uuid: node_uuid.to_owned(),
                    environment: RuntimeEnvironment::Production,
                    tenant_scope_hash: "1".repeat(64),
                },
                snapshot_uuid.to_owned(),
                "2026-07-22T00:00:00Z".to_owned(),
                vec![runtime_descriptor("site-100", &"1".repeat(64))],
            )
            .await
            .expect("enqueue target assignment");
    }
    sqlx::query(
        "UPDATE deploy_runtime_assignment SET trigger_app_revision_id = 200
         WHERE snapshot_uuid IN ('snapshot-a', 'snapshot-b')",
    )
    .execute(&pool)
    .await
    .expect("bind assignments to the site revision");

    let first = publisher
        .publish_due("worker-quorum", 10, 30)
        .await
        .expect("publish and reconcile first target");
    assert_eq!(first.published, 2);
    assert_eq!(first.observations_ingested, 1);
    assert_eq!(first.observations_pending, 1);
    assert_eq!(first.revisions_activated, 0);
    let current_after_first: Option<i64> =
        sqlx::query_scalar("SELECT current_revision_id FROM deploy_app WHERE id = 100")
            .fetch_one(&pool)
            .await
            .expect("load partially converged site");
    assert_eq!(current_after_first, None);

    web_runtime.activate("snapshot-b");
    let second = publisher
        .reconcile_observations(10)
        .await
        .expect("reconcile second target");
    assert_eq!(second.observations_checked, 1);
    assert_eq!(second.observations_ingested, 1);
    assert_eq!(second.revisions_activated, 1);
    let current_after_quorum: Option<i64> =
        sqlx::query_scalar("SELECT current_revision_id FROM deploy_app WHERE id = 100")
            .fetch_one(&pool)
            .await
            .expect("load converged site");
    assert_eq!(current_after_quorum, Some(200));
    let evidence_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM deploy_app_target_observation")
            .fetch_one(&pool)
            .await
            .expect("count observation evidence");
    assert_eq!(evidence_count, 2);
}

#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn postgres_outbox_is_durable_idempotent_and_publishable() {
    let pool = common::postgres_pool().await;
    sqlx::query(
        "INSERT INTO deploy_web_node_target (
            id, uuid, tenant_id, node_uuid, environment, tenant_scope_hash,
            status, created_at, updated_at, version
         ) VALUES (1, 'target-1', 7, 'node-1', 'production', $1,
                   'ACTIVE', '2026-07-22T00:00:00Z', '2026-07-22T00:00:00Z', 1)",
    )
    .bind("1".repeat(64))
    .execute(&pool)
    .await
    .expect("insert Web Node target");

    let repository = Arc::new(DeployRepository::new(
        pool,
        SnowflakeIdGenerator::new(1).expect("Snowflake generator"),
        common::test_secret_key(),
    ));
    let publisher = RuntimePublicationService::new(
        repository.clone() as Arc<dyn DeployRuntimeAssignmentRepositoryPort>,
        Arc::new(AcceptingWebRuntime),
    );
    let target = RuntimeTarget {
        target_uuid: "target-1".to_owned(),
        tenant_id: 7,
        node_uuid: "node-1".to_owned(),
        environment: RuntimeEnvironment::Production,
        tenant_scope_hash: "1".repeat(64),
    };

    let pending = publisher
        .enqueue_target_state(
            &target,
            "snapshot-1".to_owned(),
            "2026-07-22T00:00:00Z".to_owned(),
            vec![],
        )
        .await
        .expect("persist pending assignment");
    assert_eq!(pending.generation, 1);
    assert_eq!(
        pending.publish_status,
        RuntimeAssignmentPublishStatus::Pending
    );

    let replay = publisher
        .enqueue_target_state(
            &target,
            "snapshot-unused".to_owned(),
            "2026-07-22T00:00:00Z".to_owned(),
            vec![],
        )
        .await
        .expect("same desired state is idempotent");
    assert_eq!(replay.assignment_uuid, pending.assignment_uuid);

    let batch = publisher
        .publish_due("worker-1", 10, 30)
        .await
        .expect("publish durable assignment");
    assert_eq!(batch.claimed, 1);
    assert_eq!(batch.published, 1);
    assert_eq!(batch.failed, 0);
    let published = repository
        .latest_runtime_assignment("target-1")
        .await
        .expect("reload assignment")
        .expect("assignment exists");
    assert_eq!(
        published.publish_status,
        RuntimeAssignmentPublishStatus::Published
    );
    assert_eq!(published.snapshot_sha256, pending.snapshot_sha256);
}

#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn provider_event_delivery_failure_prevents_web_publication() {
    let pool = common::postgres_pool().await;
    sqlx::query(
        "INSERT INTO deploy_web_node_target (
            id, uuid, tenant_id, node_uuid, environment, tenant_scope_hash,
            status, created_at, updated_at, version
         ) VALUES (1, 'target-1', 7, 'node-1', 'production', $1,
                   'ACTIVE', '2026-07-22T00:00:00Z', '2026-07-22T00:00:00Z', 1)",
    )
    .bind("1".repeat(64))
    .execute(&pool)
    .await
    .expect("insert Web Node target");
    let repository = Arc::new(DeployRepository::new(
        pool.clone(),
        SnowflakeIdGenerator::new(1).expect("Snowflake generator"),
        common::test_secret_key(),
    ));
    let web_runtime = Arc::new(CountingWebRuntime::default());
    let publisher = RuntimePublicationService::new_with_provider_event_delivery(
        repository.clone(),
        web_runtime.clone(),
        Arc::new(FailingProviderEventDelivery),
    );
    publisher
        .enqueue_target_state(
            &RuntimeTarget {
                target_uuid: "target-1".to_owned(),
                tenant_id: 7,
                node_uuid: "node-1".to_owned(),
                environment: RuntimeEnvironment::Production,
                tenant_scope_hash: "1".repeat(64),
            },
            "snapshot-1".to_owned(),
            "2026-07-22T00:00:00Z".to_owned(),
            vec![],
        )
        .await
        .expect("persist pending assignment");

    let batch = publisher
        .publish_due("worker-1", 10, 30)
        .await
        .expect("record provider delivery failure");
    assert_eq!(batch.claimed, 1);
    assert_eq!(batch.published, 0);
    assert_eq!(batch.failed, 1);
    assert_eq!(web_runtime.calls.load(Ordering::SeqCst), 0);
    let row = sqlx::query(
        "SELECT publish_status, last_error_code FROM deploy_runtime_assignment WHERE uuid = $1",
    )
    .bind(
        repository
            .latest_runtime_assignment("target-1")
            .await
            .unwrap()
            .unwrap()
            .assignment_uuid,
    )
    .fetch_one(&pool)
    .await
    .expect("load failed assignment");
    assert_eq!(row.get::<String, _>("publish_status"), "FAILED");
    assert_eq!(
        row.get::<String, _>("last_error_code"),
        "PROVIDER_EVENT_DELIVERY_UNAVAILABLE"
    );
}

#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn active_assignment_renewal_is_cursor_bounded_and_failure_isolated() {
    let pool = common::postgres_pool().await;
    let tenant_scope_hash = "1".repeat(64);
    for (id, target_uuid, node_uuid) in [
        (1_i64, "target-a", "node-a"),
        (2_i64, "target-b", "node-b"),
        (3_i64, "target-c", "node-c"),
        (4_i64, "target-d", "node-d"),
    ] {
        sqlx::query(
            "INSERT INTO deploy_web_node_target (
                id, uuid, tenant_id, node_uuid, environment, tenant_scope_hash,
                status, created_at, updated_at, version
             ) VALUES ($1, $2, 7, $3, 'production', $4,
                       'ACTIVE', '2026-07-22T00:00:00Z', '2026-07-22T00:00:00Z', 1)",
        )
        .bind(id)
        .bind(target_uuid)
        .bind(node_uuid)
        .bind(&tenant_scope_hash)
        .execute(&pool)
        .await
        .expect("insert Web Node target");
    }
    let repository = Arc::new(DeployRepository::new(
        pool,
        SnowflakeIdGenerator::new(1).expect("Snowflake generator"),
        common::test_secret_key(),
    ));
    let web_runtime = Arc::new(SelectiveActiveWebRuntime::new(
        7,
        [
            "snapshot-a".to_owned(),
            "snapshot-b".to_owned(),
            "snapshot-c".to_owned(),
            "snapshot-d".to_owned(),
        ],
    ));
    let publisher = RuntimePublicationService::new(repository.clone(), web_runtime.clone());
    for (target_uuid, node_uuid, snapshot_uuid) in [
        ("target-a", "node-a", "snapshot-a"),
        ("target-b", "node-b", "snapshot-b"),
        ("target-c", "node-c", "snapshot-c"),
        ("target-d", "node-d", "snapshot-d"),
    ] {
        publisher
            .enqueue_target_state(
                &RuntimeTarget {
                    target_uuid: target_uuid.to_owned(),
                    tenant_id: 7,
                    node_uuid: node_uuid.to_owned(),
                    environment: RuntimeEnvironment::Production,
                    tenant_scope_hash: tenant_scope_hash.clone(),
                },
                snapshot_uuid.to_owned(),
                "2026-07-22T00:00:00Z".to_owned(),
                vec![],
            )
            .await
            .expect("persist runtime assignment");
    }
    let published = publisher
        .publish_due("worker-active", 10, 30)
        .await
        .expect("publish and activate assignments");
    assert_eq!(published.published, 4);
    assert_eq!(published.revisions_activated, 0);

    let first_page = repository
        .list_active_runtime_assignments_after(None, 2)
        .await
        .expect("load first active assignment page");
    assert_eq!(
        first_page
            .iter()
            .map(|assignment| assignment.target_uuid.as_str())
            .collect::<Vec<_>>(),
        ["target-a", "target-b"]
    );
    let second_page = repository
        .list_active_runtime_assignments_after(Some("target-b"), 2)
        .await
        .expect("load second active assignment page");
    assert_eq!(
        second_page
            .iter()
            .map(|assignment| assignment.target_uuid.as_str())
            .collect::<Vec<_>>(),
        ["target-c", "target-d"]
    );

    let provider_events = Arc::new(SelectiveProviderEventDelivery::new(Some("node-b")));
    let renewal = RuntimePublicationService::new_with_provider_event_delivery(
        repository.clone(),
        web_runtime.clone(),
        provider_events.clone(),
    );
    let first = renewal
        .renew_active_provider_event_deliveries(2)
        .await
        .expect("renew first active page");
    assert_eq!(first.provider_event_assignments_checked, 2);
    assert_eq!(first.provider_event_assignments_failed, 1);
    assert_eq!(first.provider_event_deliveries_ensured, 1);
    let second = renewal
        .renew_active_provider_event_deliveries(2)
        .await
        .expect("renew second active page");
    assert_eq!(second.provider_event_assignments_checked, 2);
    assert_eq!(second.provider_event_assignments_failed, 0);
    assert_eq!(second.provider_event_deliveries_ensured, 2);
    assert_eq!(
        provider_events.visited_nodes.lock().unwrap().as_slice(),
        ["node-a", "node-b", "node-c", "node-d"]
    );

    publisher
        .enqueue_target_state(
            &RuntimeTarget {
                target_uuid: "target-b".to_owned(),
                tenant_id: 7,
                node_uuid: "node-b".to_owned(),
                environment: RuntimeEnvironment::Production,
                tenant_scope_hash: tenant_scope_hash.clone(),
            },
            "snapshot-b-next".to_owned(),
            "2026-07-22T00:01:00Z".to_owned(),
            vec![runtime_descriptor("site-b-next", &tenant_scope_hash)],
        )
        .await
        .expect("persist newer pending assignment");
    let latest_active = repository
        .list_active_runtime_assignments_after(None, 10)
        .await
        .expect("exclude target whose latest assignment is not active");
    assert_eq!(
        latest_active
            .iter()
            .map(|assignment| assignment.target_uuid.as_str())
            .collect::<Vec<_>>(),
        ["target-a", "target-c", "target-d"]
    );
}

#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn postgres_outbox_reuses_assignment_when_descriptor_order_changes() {
    let pool = common::postgres_pool().await;
    let tenant_scope_hash = "1".repeat(64);
    sqlx::query(
        "INSERT INTO deploy_web_node_target (
            id, uuid, tenant_id, node_uuid, environment, tenant_scope_hash,
            status, created_at, updated_at, version
         ) VALUES (1, 'target-1', 7, 'node-1', 'production', $1,
                   'ACTIVE', '2026-07-22T00:00:00Z', '2026-07-22T00:00:00Z', 1)",
    )
    .bind(&tenant_scope_hash)
    .execute(&pool)
    .await
    .expect("insert Web Node target");

    let repository = Arc::new(DeployRepository::new(
        pool,
        SnowflakeIdGenerator::new(1).expect("Snowflake generator"),
        common::test_secret_key(),
    ));
    let publisher = RuntimePublicationService::new(
        repository as Arc<dyn DeployRuntimeAssignmentRepositoryPort>,
        Arc::new(AcceptingWebRuntime),
    );
    let target = RuntimeTarget {
        target_uuid: "target-1".to_owned(),
        tenant_id: 7,
        node_uuid: "node-1".to_owned(),
        environment: RuntimeEnvironment::Production,
        tenant_scope_hash: tenant_scope_hash.clone(),
    };
    let site_a = runtime_descriptor("site-a", &tenant_scope_hash);
    let site_b = runtime_descriptor("site-b", &tenant_scope_hash);

    let first = publisher
        .enqueue_target_state(
            &target,
            "snapshot-1".to_owned(),
            "2026-07-22T00:00:00Z".to_owned(),
            vec![site_b.clone(), site_a.clone()],
        )
        .await
        .expect("persist ordered assignment");
    let replay = publisher
        .enqueue_target_state(
            &target,
            "snapshot-unused".to_owned(),
            "2026-07-22T00:00:01Z".to_owned(),
            vec![site_a, site_b],
        )
        .await
        .expect("reuse assignment for equivalent descriptor set");

    assert_eq!(replay.assignment_uuid, first.assignment_uuid);
    assert_eq!(replay.generation, first.generation);
}

#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn postgres_concurrent_mutations_are_serialized_and_idempotent() {
    let pool = common::postgres_pool().await;
    let tenant_scope_hash = "1".repeat(64);
    sqlx::query(
        "INSERT INTO deploy_web_node_target (
            id, uuid, tenant_id, node_uuid, environment, tenant_scope_hash,
            status, created_at, updated_at, version
         ) VALUES (1, 'target-1', 7, 'node-1', 'production', $1,
                   'ACTIVE', '2026-07-22T00:00:00Z', '2026-07-22T00:00:00Z', 1)",
    )
    .bind(&tenant_scope_hash)
    .execute(&pool)
    .await
    .expect("insert Web Node target");

    let repository = Arc::new(DeployRepository::new(
        pool.clone(),
        SnowflakeIdGenerator::new(1).expect("Snowflake generator"),
        common::test_secret_key(),
    ));
    let publisher = Arc::new(RuntimePublicationService::new(
        repository.clone() as Arc<dyn DeployRuntimeAssignmentRepositoryPort>,
        Arc::new(AcceptingWebRuntime),
    ));
    let target = RuntimeTarget {
        target_uuid: "target-1".to_owned(),
        tenant_id: 7,
        node_uuid: "node-1".to_owned(),
        environment: RuntimeEnvironment::Production,
        tenant_scope_hash: tenant_scope_hash.clone(),
    };
    let site_a = runtime_descriptor("site-a", &tenant_scope_hash);

    let first = publisher.enqueue_target_state(
        &target,
        "snapshot-1".to_owned(),
        "2026-07-22T00:00:00Z".to_owned(),
        vec![site_a.clone()],
    );
    let duplicate = publisher.enqueue_target_state(
        &target,
        "snapshot-duplicate".to_owned(),
        "2026-07-22T00:00:01Z".to_owned(),
        vec![site_a],
    );
    let (first, duplicate) = tokio::join!(first, duplicate);
    let first = first.expect("persist first concurrent desired state");
    let duplicate = duplicate.expect("reuse concurrent desired state");
    assert_eq!(first.assignment_uuid, duplicate.assignment_uuid);
    assert_eq!(first.generation, 1);

    let second = publisher.enqueue_target_state(
        &target,
        "snapshot-2".to_owned(),
        "2026-07-22T00:00:02Z".to_owned(),
        vec![runtime_descriptor("site-b", &tenant_scope_hash)],
    );
    let third = publisher.enqueue_target_state(
        &target,
        "snapshot-3".to_owned(),
        "2026-07-22T00:00:03Z".to_owned(),
        vec![runtime_descriptor("site-c", &tenant_scope_hash)],
    );
    let (second, third) = tokio::join!(second, third);
    let mut concurrent_generations = [
        second.expect("persist second desired state").generation,
        third.expect("persist third desired state").generation,
    ];
    concurrent_generations.sort_unstable();
    assert_eq!(concurrent_generations, [2, 3]);

    let rows = sqlx::query(
        "SELECT generation, publish_status FROM deploy_runtime_assignment ORDER BY generation",
    )
    .fetch_all(&pool)
    .await
    .expect("load serialized assignments");
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0].get::<i64, _>("generation"), 1);
    assert_eq!(rows[0].get::<String, _>("publish_status"), "SUPERSEDED");
    assert_eq!(rows[1].get::<i64, _>("generation"), 2);
    assert_eq!(rows[1].get::<String, _>("publish_status"), "SUPERSEDED");
    assert_eq!(rows[2].get::<i64, _>("generation"), 3);
    assert_eq!(rows[2].get::<String, _>("publish_status"), "PENDING");

    drop(publisher);
    drop(repository);
    pool.close().await;
}

#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn postgres_claim_lease_fences_stale_workers_and_stops_at_attempt_limit() {
    let pool = common::postgres_pool().await;
    sqlx::query(
        "INSERT INTO deploy_web_node_target (
            id, uuid, tenant_id, node_uuid, environment, tenant_scope_hash,
            status, created_at, updated_at, version
         ) VALUES (1, 'target-1', 7, 'node-1', 'production', $1,
                   'ACTIVE', '2026-07-22T00:00:00Z', '2026-07-22T00:00:00Z', 1)",
    )
    .bind("1".repeat(64))
    .execute(&pool)
    .await
    .expect("insert Web Node target");
    let repository = Arc::new(DeployRepository::new(
        pool.clone(),
        SnowflakeIdGenerator::new(1).expect("Snowflake generator"),
        common::test_secret_key(),
    ));
    let publisher = RuntimePublicationService::new(
        repository.clone() as Arc<dyn DeployRuntimeAssignmentRepositoryPort>,
        Arc::new(AcceptingWebRuntime),
    );
    let target = RuntimeTarget {
        target_uuid: "target-1".to_owned(),
        tenant_id: 7,
        node_uuid: "node-1".to_owned(),
        environment: RuntimeEnvironment::Production,
        tenant_scope_hash: "1".repeat(64),
    };
    publisher
        .enqueue_target_state(
            &target,
            "snapshot-1".to_owned(),
            "2026-07-22T00:00:00Z".to_owned(),
            vec![],
        )
        .await
        .expect("persist pending assignment");

    let first_claim = repository
        .claim_due_runtime_assignments(
            10,
            "2026-07-22T00:00:00Z",
            "worker-1",
            "2026-07-22T00:00:30Z",
            20,
        )
        .await
        .expect("claim pending assignment");
    assert_eq!(first_claim.len(), 1);
    assert_eq!(first_claim[0].attempt_count, 1);
    assert_eq!(first_claim[0].lease_owner.as_deref(), Some("worker-1"));

    let early_claim = repository
        .claim_due_runtime_assignments(
            10,
            "2026-07-22T00:00:10Z",
            "worker-2",
            "2026-07-22T00:00:40Z",
            20,
        )
        .await
        .expect("skip active lease");
    assert!(early_claim.is_empty());

    let takeover = repository
        .claim_due_runtime_assignments(
            10,
            "2026-07-22T00:00:31Z",
            "worker-2",
            "2026-07-22T00:01:01Z",
            20,
        )
        .await
        .expect("take over expired lease");
    assert_eq!(takeover.len(), 1);
    assert_eq!(takeover[0].attempt_count, 2);
    assert_eq!(takeover[0].lease_owner.as_deref(), Some("worker-2"));

    assert!(repository
        .mark_runtime_assignment_failed(
            &takeover[0].assignment_uuid,
            "worker-1",
            "STALE_WORKER",
            None,
            "2026-07-22T00:00:32Z",
        )
        .await
        .is_err());
    repository
        .mark_runtime_assignment_failed(
            &takeover[0].assignment_uuid,
            "worker-2",
            "WEB_PUBLICATION_UNAVAILABLE",
            Some("2026-07-22T00:01:30Z"),
            "2026-07-22T00:00:32Z",
        )
        .await
        .expect("lease owner records failure");

    sqlx::query(
        "UPDATE deploy_runtime_assignment
         SET attempt_count = 20, next_attempt_at = '2026-07-22T00:01:30Z'
         WHERE uuid = $1",
    )
    .bind(&takeover[0].assignment_uuid)
    .execute(&pool)
    .await
    .expect("set maximum attempts");
    let exhausted = repository
        .claim_due_runtime_assignments(
            10,
            "2026-07-22T00:02:00Z",
            "worker-3",
            "2026-07-22T00:02:30Z",
            20,
        )
        .await
        .expect("scan exhausted assignment");
    assert!(exhausted.is_empty());
}

#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn postgres_serializes_mutations_and_fences_assignment_leases() {
    let pool = common::postgres_pool().await;

    let tenant_scope_hash = "1".repeat(64);
    sqlx::query(
        "INSERT INTO deploy_web_node_target (
            id, uuid, tenant_id, node_uuid, environment, tenant_scope_hash,
            status, created_at, updated_at, version
         ) VALUES (1, 'target-1', 7, 'node-1', 'production', $1,
                   'ACTIVE', '2026-07-22T00:00:00Z', '2026-07-22T00:00:00Z', 1)",
    )
    .bind(&tenant_scope_hash)
    .execute(&pool)
    .await
    .expect("insert PostgreSQL Web Node target");

    let repository = Arc::new(DeployRepository::new(
        pool.clone(),
        SnowflakeIdGenerator::new(1).expect("Snowflake generator"),
        common::test_secret_key(),
    ));
    let publisher = Arc::new(RuntimePublicationService::new(
        repository.clone() as Arc<dyn DeployRuntimeAssignmentRepositoryPort>,
        Arc::new(AcceptingWebRuntime),
    ));
    let target = RuntimeTarget {
        target_uuid: "target-1".to_owned(),
        tenant_id: 7,
        node_uuid: "node-1".to_owned(),
        environment: RuntimeEnvironment::Production,
        tenant_scope_hash: tenant_scope_hash.clone(),
    };

    let site_a = runtime_descriptor("site-a", &tenant_scope_hash);
    let first = publisher.enqueue_target_state(
        &target,
        "snapshot-1".to_owned(),
        "2026-07-22T00:00:00Z".to_owned(),
        vec![site_a.clone()],
    );
    let duplicate = publisher.enqueue_target_state(
        &target,
        "snapshot-duplicate".to_owned(),
        "2026-07-22T00:00:01Z".to_owned(),
        vec![site_a],
    );
    let (first, duplicate) = tokio::join!(first, duplicate);
    let first = first.expect("persist first PostgreSQL desired state");
    let duplicate = duplicate.expect("reuse concurrent PostgreSQL desired state");
    assert_eq!(first.assignment_uuid, duplicate.assignment_uuid);
    assert_eq!(first.generation, 1);

    let second = publisher.enqueue_target_state(
        &target,
        "snapshot-2".to_owned(),
        "2026-07-22T00:00:02Z".to_owned(),
        vec![runtime_descriptor("site-b", &tenant_scope_hash)],
    );
    let third = publisher.enqueue_target_state(
        &target,
        "snapshot-3".to_owned(),
        "2026-07-22T00:00:03Z".to_owned(),
        vec![runtime_descriptor("site-c", &tenant_scope_hash)],
    );
    let (second, third) = tokio::join!(second, third);
    let mut concurrent_generations = [
        second
            .expect("persist second PostgreSQL desired state")
            .generation,
        third
            .expect("persist third PostgreSQL desired state")
            .generation,
    ];
    concurrent_generations.sort_unstable();
    assert_eq!(concurrent_generations, [2, 3]);

    let rows = sqlx::query(
        "SELECT generation, publish_status
         FROM deploy_runtime_assignment
         ORDER BY generation",
    )
    .fetch_all(&pool)
    .await
    .expect("load PostgreSQL serialized assignments");
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0].get::<i64, _>("generation"), 1);
    assert_eq!(rows[0].get::<String, _>("publish_status"), "SUPERSEDED");
    assert_eq!(rows[1].get::<i64, _>("generation"), 2);
    assert_eq!(rows[1].get::<String, _>("publish_status"), "SUPERSEDED");
    assert_eq!(rows[2].get::<i64, _>("generation"), 3);
    assert_eq!(rows[2].get::<String, _>("publish_status"), "PENDING");

    let first_claim = repository.claim_due_runtime_assignments(
        10,
        "2026-07-22T00:00:04Z",
        "worker-1",
        "2026-07-22T00:00:34Z",
        20,
    );
    let competing_claim = repository.claim_due_runtime_assignments(
        10,
        "2026-07-22T00:00:04Z",
        "worker-2",
        "2026-07-22T00:00:34Z",
        20,
    );
    let (first_claim, competing_claim) = tokio::join!(first_claim, competing_claim);
    let first_claim = first_claim.expect("claim PostgreSQL pending assignment");
    let competing_claim = competing_claim.expect("run competing PostgreSQL claim");
    assert_eq!(first_claim.len() + competing_claim.len(), 1);
    let claimed = first_claim
        .first()
        .or_else(|| competing_claim.first())
        .expect("one PostgreSQL worker owns the assignment");
    let first_owner = claimed
        .lease_owner
        .as_deref()
        .expect("claimed assignment has lease owner");
    assert_eq!(claimed.attempt_count, 1);

    let active_lease_claim = repository
        .claim_due_runtime_assignments(
            10,
            "2026-07-22T00:00:20Z",
            "worker-3",
            "2026-07-22T00:00:50Z",
            20,
        )
        .await
        .expect("skip active PostgreSQL lease");
    assert!(active_lease_claim.is_empty());

    let takeover = repository
        .claim_due_runtime_assignments(
            10,
            "2026-07-22T00:00:35Z",
            "worker-3",
            "2026-07-22T00:01:05Z",
            20,
        )
        .await
        .expect("take over expired PostgreSQL lease");
    assert_eq!(takeover.len(), 1);
    assert_eq!(takeover[0].attempt_count, 2);
    assert_eq!(takeover[0].lease_owner.as_deref(), Some("worker-3"));

    assert!(repository
        .mark_runtime_assignment_failed(
            &takeover[0].assignment_uuid,
            first_owner,
            "STALE_WORKER",
            None,
            "2026-07-22T00:00:36Z",
        )
        .await
        .is_err());
    repository
        .mark_runtime_assignment_failed(
            &takeover[0].assignment_uuid,
            "worker-3",
            "WEB_PUBLICATION_UNAVAILABLE",
            Some("2026-07-22T00:01:30Z"),
            "2026-07-22T00:00:36Z",
        )
        .await
        .expect("PostgreSQL lease owner records failure");

    sqlx::query(
        "UPDATE deploy_runtime_assignment
         SET attempt_count = 20, next_attempt_at = '2026-07-22T00:01:30Z'
         WHERE uuid = $1",
    )
    .bind(&takeover[0].assignment_uuid)
    .execute(&pool)
    .await
    .expect("set PostgreSQL maximum attempts");
    let exhausted = repository
        .claim_due_runtime_assignments(
            10,
            "2026-07-22T00:02:00Z",
            "worker-4",
            "2026-07-22T00:02:30Z",
            20,
        )
        .await
        .expect("scan exhausted PostgreSQL assignment");
    assert!(exhausted.is_empty());

    sqlx::query(
        "UPDATE deploy_runtime_assignment
         SET publish_status = 'PUBLISHED', remote_assignment_uuid = 'web-assignment-postgres',
             attempt_count = 3, next_attempt_at = NULL, last_error_code = NULL,
             published_at = '2026-07-22T00:02:01Z', updated_at = '2026-07-22T00:02:01Z'
         WHERE uuid = $1",
    )
    .bind(&takeover[0].assignment_uuid)
    .execute(&pool)
    .await
    .expect("mark PostgreSQL assignment published for renewal scan");
    sqlx::query(
        "INSERT INTO deploy_app_target_observation (
            id, uuid, tenant_id, app_id, site_revision_id, node_target_id,
            runtime_assignment_id, remote_observation_uuid, remote_assignment_uuid,
            generation, snapshot_uuid, snapshot_sha256, environment, state,
            node_version, reason_code, detail, observed_at, ingested_at, created_at
         ) SELECT
            900, '01900000-0000-7000-8000-000000000900', 7, NULL, NULL, 1,
            a.id, 'observation-postgres-active', 'web-assignment-postgres',
            a.generation, a.snapshot_uuid, a.snapshot_sha256, t.environment, 'ACTIVE',
            'test', NULL, NULL,
            '2026-07-22T00:02:02Z', '2026-07-22T00:02:02Z', '2026-07-22T00:02:02Z'
         FROM deploy_runtime_assignment a
         INNER JOIN deploy_web_node_target t ON t.id = a.node_target_id
         WHERE a.uuid = $1",
    )
    .bind(&takeover[0].assignment_uuid)
    .execute(&pool)
    .await
    .expect("insert PostgreSQL ACTIVE observation");
    let active = repository
        .list_active_runtime_assignments_after(None, 1)
        .await
        .expect("scan PostgreSQL active assignment renewal page");
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].target_uuid, "target-1");
    assert!(repository
        .list_active_runtime_assignments_after(Some("target-1"), 1)
        .await
        .expect("advance PostgreSQL active assignment cursor")
        .is_empty());

    drop(publisher);
    drop(repository);
    pool.close().await;
}
