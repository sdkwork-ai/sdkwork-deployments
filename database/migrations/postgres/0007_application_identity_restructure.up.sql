-- sdkwork:migration
-- id: 0007_application_identity_restructure
-- engine: postgres
-- module: sdkwork-deployments
-- purpose: First phase of the application-factory restructure
--   (APPLICATION_FACTORY_DB_RESTRUCTURE_PLAN.md).
--
--   The baseline now declares the end state directly: `deploy_application_scaffold`
--   (+ `_category`, `_version`) replaces the storefront template family, and
--   `deploy_build_recipe` replaces `deploy_build_template`. Because the baseline is
--   also the contract source for this module, the rename must live there rather
--   than in a forward `RENAME` migration.
--
--   This migration therefore only removes the pre-restructure duplicates that the
--   historical replay migrations (0001-0006) still create on a fresh install.
--   Nothing was ever released, so those tables carry no production rows.
-- reversible: false
-- rollback: forward-fix (the legacy tables are removed; a database that still
--   needs their rows must be exported before this migration runs, so no down
--   migration is offered)
-- transactional: true
-- lock: ACCESS EXCLUSIVE on the dropped legacy tables
-- lock_timeout: 2s
-- statement_timeout: 60s

BEGIN;

-- Order matters: the version table references the template table.
DROP TABLE IF EXISTS deploy_app_template_version;
DROP TABLE IF EXISTS deploy_app_template;
DROP TABLE IF EXISTS deploy_app_template_category;
DROP TABLE IF EXISTS deploy_build_template;

COMMIT;
