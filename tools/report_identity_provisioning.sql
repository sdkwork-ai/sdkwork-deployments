-- report_identity_provisioning.sql
--
-- Read-only reconciliation report for deployment-application identity
-- provisioning (migration 0008: `deploy_app.identity_status` and
-- `deploy_app.identity_provisioning_key`).
--
-- This file is a reporting tool, not a migration and not a repair script. It
-- executes no writes; the surrounding transaction is declared READ ONLY so an
-- accidental write would fail instead of mutating the inventory.
--
-- Run:
--   psql "$SDKWORK_DATABASE_URL" -v ON_ERROR_STOP=1 \
--        -f tools/report_identity_provisioning.sql
--
-- Column provenance:
--   deploy_app.id, uuid, tenant_id, slug, updated_at, deleted_at
--       -- database/ddl/baseline/postgres/0001_deploy_baseline.sql
--   deploy_app.identity_status, deploy_app.identity_provisioning_key
--       -- database/migrations/postgres/0008_deploy_app_identity_provisioning.up.sql
--
-- Every query excludes soft-deleted rows: `deleted_at IS NOT NULL` is not an
-- open reconciliation item. To scope a query to one tenant, append
-- `AND tenant_id = <tenant_id>` to its WHERE clause.

BEGIN;
SET TRANSACTION READ ONLY;

-- 1. Inventory by provisioning state. After 0008 every row has one of
--    pending_identity / active / failed.
SELECT identity_status,
       count(*) AS application_count
  FROM deploy_app
 WHERE deleted_at IS NULL
 GROUP BY identity_status
 ORDER BY identity_status;

-- 2. Applications whose identity is still pending or has failed: the actionable
--    set. Ordered oldest-first so a stuck row surfaces before a fresh one.
SELECT id,
       uuid,
       tenant_id,
       slug,
       identity_status,
       identity_provisioning_key,
       created_at,
       updated_at
  FROM deploy_app
 WHERE deleted_at IS NULL
   AND identity_status IN ('pending_identity', 'failed')
 ORDER BY identity_status, updated_at, id;

-- 3. Anomaly: an application that reports an active identity but never recorded
--    the idempotency key the provisioning attempt uses. The key is claimed
--    before the IAM call (`begin_app_identity_provisioning`), so this row cannot
--    have come from the normal create path; report it for investigation rather
--    than silently treating it as healthy.
SELECT id,
       uuid,
       tenant_id,
       slug,
       updated_at
  FROM deploy_app
 WHERE deleted_at IS NULL
   AND identity_status = 'active'
   AND identity_provisioning_key IS NULL
 ORDER BY updated_at, id;

COMMIT;
