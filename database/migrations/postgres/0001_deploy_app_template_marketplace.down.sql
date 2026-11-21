-- sdkwork:migration
-- id: 0001_deploy_app_template_marketplace
-- engine: postgres
-- module: sdkwork-deploy
-- purpose: Revert the marketplace catalog tables added by the 0001 up
--   migration. Tables are dropped in reverse dependency order and every drop is
--   guarded by IF EXISTS without CASCADE, so an unrelated dependent object
--   fails loudly instead of being removed with the table.
-- reversible: true
-- rollback: down-migration
-- transactional: true
-- lock: lightweight
-- lock_timeout: 2s
-- statement_timeout: 30s

BEGIN;

DROP TABLE IF EXISTS deploy_app_template_version;

DROP TABLE IF EXISTS deploy_app_template;

DROP TABLE IF EXISTS deploy_app_template_category;

COMMIT;
