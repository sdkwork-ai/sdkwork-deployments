-- sdkwork:migration
-- id: 0012_deploy_application_resource_link_reconcile
-- engine: postgres
-- module: sdkwork-deploy
-- purpose: Revert 0012_deploy_application_resource_link_reconcile.
--
--   The previous `uk_deploy_application_resource_link_default` definition is
--   restored exactly as the baseline declared it (keyed on `provider_code`
--   without COALESCE), because a rollback has to leave the schema it found. The
--   NULL hole that definition carries comes back with it — which is why the down
--   migration is only correct together with reverting the code that relies on the
--   default being unique.
-- reversible: down-migration of 0012
-- rollback: forward-fix by re-applying 0012
-- transactional: true
-- lock: ACCESS EXCLUSIVE on deploy_application_resource_link
-- lock_timeout: 2s
-- statement_timeout: 60s

BEGIN;

ALTER TABLE deploy_application_resource_link
    DROP CONSTRAINT IF EXISTS chk_deploy_application_resource_link_default_kind;
ALTER TABLE deploy_application_resource_link
    DROP CONSTRAINT IF EXISTS chk_deploy_application_resource_link_provider;
ALTER TABLE deploy_application_resource_link
    DROP CONSTRAINT IF EXISTS chk_deploy_application_resource_link_ref;
ALTER TABLE deploy_application_resource_link
    DROP CONSTRAINT IF EXISTS chk_deploy_application_resource_link_environment;

DROP INDEX IF EXISTS uk_deploy_application_resource_link_default;
CREATE UNIQUE INDEX IF NOT EXISTS uk_deploy_application_resource_link_default
    ON deploy_application_resource_link (app_id, env_key, provider_code)
    WHERE is_default AND resource_kind = 'payment_account' AND deleted_at IS NULL;

COMMIT;
