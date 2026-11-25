-- sdkwork:migration
-- id: 0022_deploy_dns_zone_verification_file_updated_by_reconciliation
-- engine: postgres
-- module: deploy
-- purpose: Restore the `updated_by` audit column on
--   deploy_dns_zone_verification_file, which upgraded databases never received.
--
--   The consolidated baseline creates the table with the full audit pair
--   (created_by, updated_by), but an existing deployment got the table from
--   migration 0006, whose CREATE TABLE IF NOT EXISTS mirrored the older shape
--   and therefore silently kept the pre-0006 table as it was. Fresh installs
--   have both columns; upgraded ones have only created_by.
--
--   The verification-file upsert writes the acting subject into both audit
--   columns (see sdkwork-intelligence-deploy-repository-sqlx, the
--   `upsert deploy_dns_zone_verification_file` statement), so every create on
--   such a database failed with
--   `column "updated_by" of relation "deploy_dns_zone_verification_file" does
--   not exist` and answered HTTP 500.
--
--   ADD COLUMN IF NOT EXISTS is idempotent: on a database provisioned from the
--   current baseline the statement is a no-op, so the migration is safe to run
--   on every deployment. The column is nullable, matching the baseline and the
--   other audit columns on this table (rows created by a system actor carry
--   NULL).

ALTER TABLE deploy_dns_zone_verification_file
    ADD COLUMN IF NOT EXISTS updated_by BIGINT;

COMMENT ON COLUMN deploy_dns_zone_verification_file.updated_by IS
    '最后修改该行的操作主体；NULL 表示由系统写入或早于本列的历史行，与 created_by 同义';
