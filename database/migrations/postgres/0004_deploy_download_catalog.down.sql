-- sdkwork:migration
-- id: 0004_deploy_download_catalog
-- engine: postgres
-- module: deploy
-- description: Reverts the download catalog tables added by the 0004 up
--   migration. Tables are dropped in reverse dependency order and every drop is
--   guarded by IF EXISTS without CASCADE, so an unrelated dependent object
--   fails loudly instead of being removed with the table.
-- reversible: true
-- rollback: down-migration
-- transactional: true
-- lock: lightweight
-- lock_timeout: 2s
-- statement_timeout: 60s

BEGIN;

DROP TABLE IF EXISTS deploy_download_update;

DROP TABLE IF EXISTS deploy_download_item;

DROP TABLE IF EXISTS deploy_download_card;

DROP TABLE IF EXISTS deploy_download_catalog;

COMMIT;
