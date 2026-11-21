-- sdkwork:migration
-- id: 0009_upload_session_ref_idempotency_per_application
-- engine: postgres
-- module: sdkwork-deploy
-- purpose: The upload session idempotency index keyed only on (tenant_id,
--   idempotency_key). Two applications of the same tenant that happen to choose the
--   same key collided, and the second one received a conflict for a session it never
--   created. The key is application-scoped, so the index joins app_id.
--
--   NULL app_id stays valid for tenant-scoped sessions and keeps its previous
--   grouping through COALESCE(app_id, 0).
-- reversible: true
-- rollback: down-migration (the paired down migration restores the original index)
-- transactional: true
-- lock: CREATE INDEX without CONCURRENTLY takes a share lock on the table
-- lock_timeout: 2s
-- statement_timeout: 60s

BEGIN;

DROP INDEX IF EXISTS uk_deploy_upload_session_ref_idempotency;

CREATE UNIQUE INDEX IF NOT EXISTS uk_deploy_upload_session_ref_idempotency
    ON deploy_upload_session_ref (tenant_id, COALESCE(app_id, 0), idempotency_key);

COMMIT;
