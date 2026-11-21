-- sdkwork:migration
-- id: 0008_deploy_app_identity_provisioning
-- engine: postgres
-- module: sdkwork-deploy
-- purpose: Reverse the deployment identity-provisioning columns.
-- reversible: true
-- rollback: down-migration
-- transactional: true
-- lock: ACCESS EXCLUSIVE on deploy_app
-- lock_timeout: 2s
-- statement_timeout: 60s

BEGIN;

DROP INDEX IF EXISTS idx_deploy_app_identity_status;
DROP INDEX IF EXISTS uk_deploy_app_identity_provisioning_key;

ALTER TABLE deploy_app
    DROP CONSTRAINT IF EXISTS chk_deploy_app_identity_status;

ALTER TABLE deploy_app
    DROP COLUMN IF EXISTS identity_provisioning_key,
    DROP COLUMN IF EXISTS identity_status;

COMMIT;
