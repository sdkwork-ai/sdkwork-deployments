-- sdkwork:migration
-- id: 0013_deploy_application_resource_link_backfill
-- engine: postgres
-- module: sdkwork-deploy
-- purpose: Revert 0013_deploy_application_resource_link_backfill.
--
--   Deletes exactly the rows the backfill created, identified by the provenance
--   marker it stamped into `metadata.source`. Rows written through the API are
--   never touched, and `deploy_app.metadata.integrations` is never modified — the
--   source document is the reason the rows can be re-derivable at all.
-- reversible: down-migration of 0013 (data only; the table and its constraints
--   from 0012 stay)
-- rollback: forward-fix by re-applying 0013
-- transactional: true
-- lock: row locks on the rows being deleted
-- lock_timeout: 2s
-- statement_timeout: 60s

BEGIN;

DO $rollback$
DECLARE
    deleted_count BIGINT;
BEGIN
    DELETE FROM deploy_application_resource_link
    WHERE metadata ->> 'source' = 'deploy_app.metadata.integrations'
      AND metadata ->> 'sourceSchema' = 'sdkwork.application.integrations/v1';

    GET DIAGNOSTICS deleted_count = ROW_COUNT;
    RAISE NOTICE 'deploy_application_resource_link backfill rolled back: % row(s) removed', deleted_count;
END
$rollback$;

COMMIT;
