-- sdkwork:migration
-- id: 0014_deploy_app_ownership_alignment_and_app_kind
-- engine: postgres
-- module: sdkwork-deploy
-- purpose: Revert 0014_deploy_app_ownership_alignment_and_app_kind.
--
--   Drops the two constraints and the two indexes the up migration added and
--   restores the weaker `chk_deploy_app_owner_pointer` that preceded it. No row
--   is touched in either direction: the up migration only constrained future
--   writes (the `app_kind` half is `NOT VALID`), so reverting constraints cannot
--   lose data.
--
--   Restoring the old constraint can fail if rows written while 0014 was in
--   force are now incompatible with it -- which is the point of the stronger
--   shape. That failure is reported rather than worked around; fix forward.
-- reversible: true
-- rollback: forward-fix by re-applying 0014 after resolving the reported rows
-- transactional: true
-- lock: access-exclusive for the ALTER TABLE / DROP INDEX statements
-- lock_timeout: 5s
-- statement_timeout: 60s

BEGIN;

ALTER TABLE deploy_app
    DROP CONSTRAINT IF EXISTS chk_deploy_app_app_kind;

ALTER TABLE deploy_app
    DROP CONSTRAINT IF EXISTS chk_deploy_app_ownership_alignment;

DO $ownership_pointer$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conname = 'chk_deploy_app_owner_pointer'
          AND conrelid = 'deploy_app'::regclass
    ) THEN
        ALTER TABLE deploy_app
            ADD CONSTRAINT chk_deploy_app_owner_pointer CHECK (
                (owner_type = 'USER' AND user_id IS NOT NULL)
                OR (owner_type = 'ORGANIZATION' AND organization_id <> 0)
                OR (owner_type IN ('PLATFORM', 'TENANT') AND user_id IS NULL)
            );
    END IF;
END
$ownership_pointer$;

DROP INDEX IF EXISTS idx_deploy_app_tenant_ownership_kind;
DROP INDEX IF EXISTS idx_deploy_app_tenant_organization_kind;

COMMIT;
