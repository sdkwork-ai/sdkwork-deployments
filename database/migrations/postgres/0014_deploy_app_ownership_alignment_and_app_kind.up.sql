-- sdkwork:migration
-- id: 0014_deploy_app_ownership_alignment_and_app_kind
-- engine: postgres
-- module: sdkwork-deploy
-- purpose: Make the three application registration modes explicit and
--   enforceable, and close `app_kind` to a documented endpoint vocabulary.
--
--   Ownership. `deploy_app.owner_type` already names the ownership level
--   (`PLATFORM` / `TENANT` / `ORGANIZATION` / `USER`) and `deploy_app` already
--   carries `organization_id` and `user_id`, but the shipped
--   `chk_deploy_app_owner_pointer` only checked each pointer in isolation. A
--   `TENANT` row could therefore carry a real organization, and a `USER` row
--   could keep one, which made "tenant application" and "organization
--   application" the same row under two labels and left the admin ledger unable
--   to answer which of the three modes a row is in. This migration replaces it
--   with three mutually exclusive shapes:
--
--     TENANT       organization_id = 0,  user_id IS NULL
--     ORGANIZATION organization_id > 0,  user_id IS NULL
--     USER         organization_id = 0,  user_id IS NOT NULL
--
--   `PLATFORM` is platform-internal and shares the TENANT shape because its
--   scope is the platform, not an organization.
--
--   Existing rows are **reported, never rewritten**. A schema that carries a
--   row not matching one of the shapes is not guessed at: the migration raises
--   with the conflicting count and up to twenty `(uuid, owner_type,
--   organization_id, user_id)` tuples, and the operator decides per row whether
--   the intent was an organization app (`owner_type = 'ORGANIZATION'`) or a
--   tenant app that should lose its organization pointer (`organization_id =
--   0`). Re-running after the repair applies the constraint.
--
--   Endpoint kind. `app_kind` was `VARCHAR(32)` with no constraint, so legacy
--   values such as `WEB` and `MOBILE` could be written even though the contract
--   only ever produced the canonical set. The constraint added here is the same
--   closed set the contract enum declares (`STATIC_WEB`, `SPA_WEB`, `H5_WEB`,
--   `PC_WEB`, `API_SERVICE`, `WECHAT_MINIPROGRAM`, `DOUYIN_MINIPROGRAM`,
--   `IOS_APP`, `ANDROID_APP`, `HARMONYOS_APP`, `FLUTTER_APP`, `DESKTOP_APP`).
--   It is added `NOT VALID` and legacy values are reported: new and updated
--   rows must be canonical immediately, while pre-existing rows stay readable
--   until the operator normalizes them, at which point
--   `ALTER TABLE deploy_app VALIDATE CONSTRAINT chk_deploy_app_app_kind;`
--   completes the cutover. This is deliberately not a silent rewrite.
--
--   Indexes. The operator ledger filters by `(tenant, ownership mode,
--   app_kind)` and by `(tenant, organization, app_kind)`; both get an index.
--
-- reversible: true
-- rollback: down-migration (drops the two new constraints and the two new
--   indexes and restores `chk_deploy_app_owner_pointer`; no row is touched in
--   either direction)
-- transactional: true
-- lock: access-exclusive for the ALTER TABLE / CREATE INDEX statements
-- lock_timeout: 5s
-- statement_timeout: 120s

BEGIN;

-- 1. Report -- and refuse to guess at -- ownership rows that fit none of the
--    three shapes. The exception text is the report; it names the exact rows.
--
--    Soft-deleted rows are included on purpose: the CHECK constraint is
--    table-wide, not partial, so a conflicting `deleted_at IS NOT NULL` row would
--    otherwise make the later `ADD CONSTRAINT` fail with no report at all.
DO $ownership_report$
DECLARE
    conflicting BIGINT;
    sample TEXT;
BEGIN
    WITH conflicts AS (
        SELECT id, uuid, owner_type, organization_id, user_id, deleted_at
        FROM deploy_app
        WHERE NOT (
              (owner_type IN ('PLATFORM', 'TENANT')
                  AND organization_id = 0
                  AND user_id IS NULL)
              OR (owner_type = 'ORGANIZATION'
                  AND organization_id > 0
                  AND user_id IS NULL)
              OR (owner_type = 'USER'
                  AND organization_id = 0
                  AND user_id IS NOT NULL)
          )
    )
    SELECT COUNT(*) INTO conflicting FROM conflicts;

    IF conflicting > 0 THEN
        WITH conflicts AS (
            SELECT id, uuid, owner_type, organization_id, user_id, deleted_at
            FROM deploy_app
            WHERE NOT (
                  (owner_type IN ('PLATFORM', 'TENANT')
                      AND organization_id = 0
                      AND user_id IS NULL)
                  OR (owner_type = 'ORGANIZATION'
                      AND organization_id > 0
                      AND user_id IS NULL)
                  OR (owner_type = 'USER'
                      AND organization_id = 0
                      AND user_id IS NOT NULL)
              )
            ORDER BY id
            LIMIT 20
        )
        SELECT string_agg(
                   format('uuid=%s owner_type=%s organization_id=%s user_id=%s deleted=%s',
                          uuid, owner_type, organization_id,
                          COALESCE(user_id::text, 'NULL'),
                          (deleted_at IS NOT NULL)::text),
                   '; ' ORDER BY id)
        INTO sample
        FROM conflicts;

        RAISE EXCEPTION
            'deploy_app ownership conflicts: % row(s) match none of TENANT(organization_id=0,user_id NULL), ORGANIZATION(organization_id>0,user_id NULL), USER(organization_id=0,user_id NOT NULL). Samples: %',
            conflicting, sample
            USING HINT = 'Repair each row deliberately: set owner_type = ''ORGANIZATION'' where the app really belongs to that organization, or organization_id = 0 where it is a tenant/user application. Soft-deleted rows need the same decision (or a hard delete). This migration never rewrites ownership. Re-run after the repair.';
    END IF;
END
$ownership_report$;

-- 2. Replace the weaker pointer constraint with the three-shape alignment.
ALTER TABLE deploy_app
    DROP CONSTRAINT IF EXISTS chk_deploy_app_owner_pointer;

DO $ownership_constraint$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conname = 'chk_deploy_app_ownership_alignment'
          AND conrelid = 'deploy_app'::regclass
    ) THEN
        ALTER TABLE deploy_app
            ADD CONSTRAINT chk_deploy_app_ownership_alignment CHECK (
                (owner_type IN ('PLATFORM', 'TENANT')
                    AND organization_id = 0
                    AND user_id IS NULL)
                OR (owner_type = 'ORGANIZATION'
                    AND organization_id > 0
                    AND user_id IS NULL)
                OR (owner_type = 'USER'
                    AND organization_id = 0
                    AND user_id IS NOT NULL)
            );
    END IF;
END
$ownership_constraint$;

-- 3. Report every non-canonical `app_kind` before constraining new writes. The
--    report is a WARNING rather than an exception because the legacy values are
--    readable; only new/updated rows are held to the canonical vocabulary via
--    the NOT VALID constraint below.
DO $app_kind_report$
DECLARE
    legacy RECORD;
BEGIN
    FOR legacy IN
        SELECT app_kind, COUNT(*) AS rows
        FROM deploy_app
        WHERE app_kind NOT IN (
            'STATIC_WEB', 'SPA_WEB', 'H5_WEB', 'PC_WEB',
            'API_SERVICE',
            'WECHAT_MINIPROGRAM', 'DOUYIN_MINIPROGRAM',
            'IOS_APP', 'ANDROID_APP', 'HARMONYOS_APP',
            'FLUTTER_APP', 'DESKTOP_APP'
        )
        GROUP BY app_kind
        ORDER BY app_kind
    LOOP
        RAISE WARNING 'deploy_app.app_kind % has % legacy row(s); normalize them to a canonical endpoint kind before VALIDATE CONSTRAINT chk_deploy_app_app_kind',
            legacy.app_kind, legacy.rows;
    END LOOP;
END
$app_kind_report$;

DO $app_kind_constraint$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conname = 'chk_deploy_app_app_kind'
          AND conrelid = 'deploy_app'::regclass
    ) THEN
        ALTER TABLE deploy_app
            ADD CONSTRAINT chk_deploy_app_app_kind CHECK (
                app_kind IN (
                    'STATIC_WEB', 'SPA_WEB', 'H5_WEB', 'PC_WEB',
                    'API_SERVICE',
                    'WECHAT_MINIPROGRAM', 'DOUYIN_MINIPROGRAM',
                    'IOS_APP', 'ANDROID_APP', 'HARMONYOS_APP',
                    'FLUTTER_APP', 'DESKTOP_APP'
                )
            ) NOT VALID;
    END IF;
END
$app_kind_constraint$;

-- 4. Facet indexes for the admin ledger.
CREATE INDEX IF NOT EXISTS idx_deploy_app_tenant_ownership_kind
    ON deploy_app (tenant_id, owner_type, app_kind)
    WHERE deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_deploy_app_tenant_organization_kind
    ON deploy_app (tenant_id, organization_id, app_kind)
    WHERE deleted_at IS NULL;

COMMIT;
