-- sdkwork:migration
-- id: 0010_upload_session_ref_idempotency_keep_soft_delete_predicate
-- engine: postgres
-- module: sdkwork-deploy
-- purpose: 0009 moved the idempotency index onto the application but dropped the
--   baseline's `WHERE deleted_at IS NULL` predicate. Lookups ignore soft-deleted
--   sessions, so a retried request could not see the old row yet still collided
--   with it on the stronger index. This restores the predicate on the
--   application-scoped shape.
--
--   Verified against PostgreSQL 18 before and after: the soft-delete then
--   retry path fails on 0009 and succeeds here.
-- reversible: true
-- rollback: down-migration (restores the 0009 shape)
-- transactional: true
-- lock: share lock on the table
-- lock_timeout: 2s
-- statement_timeout: 60s

BEGIN;

DROP INDEX IF EXISTS uk_deploy_upload_session_ref_idempotency;

CREATE UNIQUE INDEX IF NOT EXISTS uk_deploy_upload_session_ref_idempotency
    ON deploy_upload_session_ref (tenant_id, COALESCE(app_id, 0), idempotency_key)
    WHERE deleted_at IS NULL;

COMMIT;
