-- sdkwork:migration
-- id: 0010_upload_session_ref_idempotency_keep_soft_delete_predicate
-- engine: postgres
-- module: sdkwork-deploy
-- purpose: Restore the 0009 index shape without the soft-delete predicate.
-- reversible: true
-- rollback: down-migration
-- transactional: true
-- lock: share lock on the table
-- lock_timeout: 2s
-- statement_timeout: 60s

BEGIN;

DROP INDEX IF EXISTS uk_deploy_upload_session_ref_idempotency;

CREATE UNIQUE INDEX IF NOT EXISTS uk_deploy_upload_session_ref_idempotency
    ON deploy_upload_session_ref (tenant_id, COALESCE(app_id, 0), idempotency_key);

COMMIT;
