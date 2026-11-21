-- sdkwork:migration
-- id: 0011_deploy_app_remix_source
-- engine: postgres
-- module: sdkwork-deploy
-- purpose: Restore the pre-remix schema: drop the app source table (and the
--   indexes and constraints it owns) and the `remix_enabled` column.
-- reversible: true
-- rollback: down-migration
-- transactional: true
-- lock: ACCESS EXCLUSIVE on deploy_app and deploy_app_source
-- lock_timeout: 2s
-- statement_timeout: 60s

BEGIN;

DROP TABLE IF EXISTS deploy_app_source;

DROP INDEX IF EXISTS idx_deploy_app_remix_enabled;

ALTER TABLE deploy_app
    DROP COLUMN IF EXISTS remix_enabled;

COMMIT;
