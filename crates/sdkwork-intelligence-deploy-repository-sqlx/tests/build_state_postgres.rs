//! Runner-reported build state transitions: a terminal state is final, and the
//! recorded duration is measured from the build's own start instant.
//!
//! Both behaviours used to be unreachable. `deploy_build.started_at` and
//! `finished_at` are TIMESTAMPTZ and were read as `String`; sqlx declares the
//! target type on the wire, PostgreSQL rejects `text` against `timestamptz`,
//! and `.ok()` turned that rejection into `None`. So the terminal-state guard
//! never fired — a SUCCEEDED build could be driven through the state machine
//! again — and `durationMs` was never computed. `started_at` was not even part
//! of the row the transition read.
//!
//! Requires `SDKWORK_DATABASE_TEST_POSTGRES_URL`; ignored by default like the
//! other PostgreSQL integration tests in this crate.

mod common;

use sdkwork_deploy_contract::{BuildStatus, DeployServiceErrorKind, UpdateBuildStateRequest};
use sdkwork_intelligence_deploy_service::DeployRepositoryPort;

const APP_ID: i64 = 7101;
const PLATFORM_TARGET_ID: i64 = 7102;
const BUILD_ID: i64 = 7103;

/// A transition reported by `runner-1`, which owns the build's claim.
fn transition(status: BuildStatus) -> UpdateBuildStateRequest {
    UpdateBuildStateRequest {
        build_status: status,
        runner_node_uuid: "runner-1".to_owned(),
        runner_version: None,
        log_ref: None,
        source_snapshot: None,
        quality_gate: None,
        error_code: None,
        started_at: None,
        finished_at: None,
    }
}

#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn terminal_build_state_is_final_and_duration_is_derived() {
    let repository = common::migrated_repository("sdkwork-deploy-build-state-test").await;
    sqlx::query(
        "INSERT INTO deploy_app (id, uuid, tenant_id, organization_id, name, slug, app_kind,
             app_status, created_at, updated_at)
         VALUES ($1, '00000000-0000-4000-8000-0000000000a1', 7, 9, 'build-state-app',
                 'build-state-app', 'API_SERVICE', 'ACTIVE', NOW(), NOW())",
    )
    .bind(APP_ID)
    .execute(repository.pool())
    .await
    .expect("insert app");

    common::seed_platform_target(repository.pool(), PLATFORM_TARGET_ID, APP_ID).await;
    common::seed_build(
        repository.pool(),
        BUILD_ID,
        APP_ID,
        PLATFORM_TARGET_ID,
        "1.0.0",
        0,
    )
    .await;
    // Put the build in flight. `started_at` two minutes back is what the
    // duration has to be measured from, and a NULL `finished_at` is what makes
    // the transition admissible at all.
    sqlx::query(
        "UPDATE deploy_build
         SET build_status = 'COMPILING', runner_node_uuid = 'runner-1',
             started_at = NOW() - INTERVAL '2 minutes', finished_at = NULL
         WHERE id = $1",
    )
    .bind(BUILD_ID)
    .execute(repository.pool())
    .await
    .expect("put the build in flight");

    let app_uuid: String = sqlx::query_scalar("SELECT uuid FROM deploy_app WHERE id = $1")
        .bind(APP_ID)
        .fetch_one(repository.pool())
        .await
        .expect("app uuid");
    let build_uuid: String = sqlx::query_scalar("SELECT uuid FROM deploy_build WHERE id = $1")
        .bind(BUILD_ID)
        .fetch_one(repository.pool())
        .await
        .expect("build uuid");

    let finished = repository
        .update_build_state(
            7,
            &app_uuid,
            &build_uuid,
            &transition(BuildStatus::Succeeded),
        )
        .await
        .expect("terminal transition");
    assert_eq!(finished.build_status, "SUCCEEDED");
    assert!(
        finished.finished_at.is_some(),
        "a terminal transition must record finishedAt"
    );
    assert!(
        finished.started_at.is_some(),
        "the build's own start instant must survive the transition"
    );
    let duration = finished
        .duration_ms
        .expect("a terminal transition must derive the duration");
    assert!(
        (100_000..=200_000).contains(&duration),
        "duration must be measured from the build's start instant, got {duration} ms"
    );

    // Terminal states are final. Before the fix this returned Ok and rewrote the
    // row, because the guard's precondition could never decode.
    let error = repository
        .update_build_state(7, &app_uuid, &build_uuid, &transition(BuildStatus::Failed))
        .await
        .expect_err("a finished build must not transition again");
    assert_eq!(error.kind(), DeployServiceErrorKind::Conflict);
    assert!(error.to_string().contains("terminal"));

    let stored: (String, Option<i64>) =
        sqlx::query_as("SELECT build_status, duration_ms FROM deploy_build WHERE id = $1")
            .bind(BUILD_ID)
            .fetch_one(repository.pool())
            .await
            .expect("stored build state");
    assert_eq!(
        stored.0, "SUCCEEDED",
        "the rejected transition must not write"
    );
    assert_eq!(stored.1, Some(duration));
}
