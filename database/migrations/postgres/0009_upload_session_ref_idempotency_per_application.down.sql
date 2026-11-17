-- sdkwork:migration
-- id: 0009_upload_session_ref_idempotency_per_application
-- engine: postgres
-- module: sdkwork-deploy
-- purpose: Restore the tenant-scoped idempotency index.
-- reversible: true
-- rollback: down-migration
-- transactional: true
-- lock: share lock on the table
-- lock_timeout: 2s
-- statement_timeout: 60s

BEGIN;

DROP INDEX IF EXISTS uk_deploy_upload_session_ref_idempotency;

CREATE UNIQUE INDEX IF NOT EXISTS uk_deploy_upload_session_ref_idempotency
    ON deploy_upload_session_ref (tenant_id, idempotency_key);

COMMIT;
