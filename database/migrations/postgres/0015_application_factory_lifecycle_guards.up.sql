-- sdkwork:migration
-- id: 0015_application_factory_lifecycle_guards
-- engine: postgres
-- module: deploy
-- purpose: Close the application-factory lifecycle gaps the 2026-11-20 maturity
--   review found in the deploy schema, after the code change that consumes each
--   one:
--
--   1. `deploy_app_environment.env_key` had no vocabulary constraint at all
--      (only `env_level` was checked), so a `qa`/`uat` environment could be
--      written into the table `APPLICATION_FACTORY_DEPLOYMENT_SPEC.md` section 7
--      says has exactly five values. The CHECK is added here after a pre-flight
--      report, and the documented aliases (`prod`, `dev`, `stage`) are
--      canonicalised first so a development database converges without an
--      operator guessing.
--   2. `deploy_deployment.environment` is the column the operator console
--      actually writes (the publish dialog), and it was unconstrained free text.
--      It receives the same five-value CHECK. A foreign key to
--      `deploy_app_environment(app_id, env_key)` is NOT added: the deployment row
--      may be written before its environment row exists (the console publishes
--      first and registers environments later), and requiring the row would turn
--      a publish into a two-step insert. The vocabulary CHECK is the invariant
--      this migration owns; the missing FK is recorded in section 13.
--   3. `deploy_app.app_surface` records which endpoint surface of its `app_kind`
--      the application is (a `FLUTTER_APP` is one application with several
--      platform targets, and the publish dialog cannot otherwise tell which
--      surface a target belongs to). Nullable because every existing row
--      predates the column; a fresh create writes it.
--   4. `deploy_app.uninstall_status` makes forward-only uninstall replayable
--      (`APPLICATION_FACTORY_DEPLOYMENT_SPEC.md` section 3 stage 6 and section
--      9). It is a durable stage marker, not a second status: `app_status`
--      remains the application state machine (`chk_deploy_app_status`) and this
--      column records how far the uninstall sequence got.
--
--   `deploy_app_platform_target` also receives the platform vocabulary CHECK it
--   never had. The kind -> platform pair cannot be expressed as a single-row
--   CHECK (it spans two tables), so it stays a service rule; this CHECK stops a
--   direct write from storing a platform name the matrix does not know.
--
--   The ownership-alignment guard for `deploy_app.owner_type` lives in the
--   service (`set_app_owner_repo` refuses a level change after provisioning,
--   where the installation scope is stored); no database trigger is added here
--   because the deploy module cannot see `iam_application_installation`.
-- reversible: false
-- rollback: forward-fix (the CHECKs and columns are additive and the code that
--   writes them ships with this change; dropping the columns would lose the
--   surface record and the uninstall stage marker)
-- transactional: true
-- lock: ACCESS EXCLUSIVE on deploy_app, deploy_app_environment, and
--   deploy_app_platform_target for the added columns and validated constraints
-- lock_timeout: 2s
-- statement_timeout: 60s

BEGIN;

-- 1. deploy_app: the endpoint surface and the uninstall stage marker.

ALTER TABLE deploy_app
    ADD COLUMN IF NOT EXISTS app_surface VARCHAR(24);

ALTER TABLE deploy_app
    DROP CONSTRAINT IF EXISTS chk_deploy_app_app_surface;
ALTER TABLE deploy_app
    ADD CONSTRAINT chk_deploy_app_app_surface CHECK (
        app_surface IS NULL
        OR app_surface IN (
            'H5',
            'PC',
            'MINI_PROGRAM',
            'FLUTTER',
            'IOS',
            'ANDROID',
            'HARMONYOS',
            'API_SERVICE',
            'DESKTOP',
            'STATIC',
            'OTHER'
        )
    );

COMMENT ON COLUMN deploy_app.app_surface IS
    'The endpoint surface of this application (section 5 view of app_kind). NULL on rows that predate the column; a fresh create records it so a multi-platform kind names the surface it publishes.';

ALTER TABLE deploy_app
    ADD COLUMN IF NOT EXISTS uninstall_status VARCHAR(24) NOT NULL DEFAULT 'not_started';

ALTER TABLE deploy_app
    DROP CONSTRAINT IF EXISTS chk_deploy_app_uninstall_status;
ALTER TABLE deploy_app
    ADD CONSTRAINT chk_deploy_app_uninstall_status CHECK (
        uninstall_status IN (
            'not_started',
            'pending_identity_disable',
            'identity_disabled',
            'complete',
            'failed'
        )
    );

COMMENT ON COLUMN deploy_app.uninstall_status IS
    'Forward-only uninstall stage marker (APPLICATION_FACTORY_DEPLOYMENT_SPEC.md section 3 stage 6): not_started -> pending_identity_disable -> identity_disabled -> complete, failed when the IAM disable step could not be completed. app_status remains the application state machine; this column only makes the cross-module sequence replayable.';

-- 2. Environment vocabulary: canonicalise the documented aliases, refuse
--    anything else by name, then constrain both tables that carry the value.

UPDATE deploy_app_environment
   SET env_key = CASE TRIM(LOWER(env_key))
           WHEN 'dev' THEN 'development'
           WHEN 'development' THEN 'development'
           WHEN 'test' THEN 'test'
           WHEN 'testing' THEN 'test'
           WHEN 'stage' THEN 'staging'
           WHEN 'staging' THEN 'staging'
           WHEN 'demo' THEN 'demo'
           WHEN 'prod' THEN 'production'
           WHEN 'production' THEN 'production'
           ELSE env_key
       END,
       updated_at = NOW(),
       version = version + 1
 WHERE TRIM(LOWER(env_key)) IN ('dev', 'stage', 'testing', 'prod');

DO $$
DECLARE
    offending TEXT;
BEGIN
    SELECT STRING_AGG(
               FORMAT('app_id=%s env_key=%L', app_id, env_key),
               '; ' ORDER BY app_id, env_key)
      INTO offending
      FROM deploy_app_environment
     WHERE env_key NOT IN ('development', 'test', 'staging', 'demo', 'production');

    IF offending IS NOT NULL THEN
        RAISE EXCEPTION
            '0015 cannot constrain deploy_app_environment.env_key: % row(s) carry an environment outside the five lifecycle values; rename them to one of development/test/staging/demo/production before applying this migration',
            offending
            USING ERRCODE = 'check_violation';
    END IF;
END $$;

ALTER TABLE deploy_app_environment
    DROP CONSTRAINT IF EXISTS chk_deploy_app_environment_env_key;
ALTER TABLE deploy_app_environment
    ADD CONSTRAINT chk_deploy_app_environment_env_key CHECK (
        env_key IN ('development', 'test', 'staging', 'demo', 'production')
    );

UPDATE deploy_deployment
   SET environment = CASE TRIM(LOWER(environment))
           WHEN 'dev' THEN 'development'
           WHEN 'development' THEN 'development'
           WHEN 'test' THEN 'test'
           WHEN 'testing' THEN 'test'
           WHEN 'stage' THEN 'staging'
           WHEN 'staging' THEN 'staging'
           WHEN 'demo' THEN 'demo'
           WHEN 'prod' THEN 'production'
           WHEN 'production' THEN 'production'
           ELSE environment
       END
 WHERE TRIM(LOWER(environment)) IN ('dev', 'stage', 'testing', 'prod');

DO $$
DECLARE
    offending TEXT;
BEGIN
    SELECT STRING_AGG(
               FORMAT('app_id=%s environment=%L', app_id, environment),
               '; ' ORDER BY app_id, environment)
      INTO offending
      FROM deploy_deployment
     WHERE environment NOT IN ('development', 'test', 'staging', 'demo', 'production');

    IF offending IS NOT NULL THEN
        RAISE EXCEPTION
            '0015 cannot constrain deploy_deployment.environment: % row(s) carry an environment outside the five lifecycle values; rename them before applying this migration',
            offending
            USING ERRCODE = 'check_violation';
    END IF;
END $$;

ALTER TABLE deploy_deployment
    DROP CONSTRAINT IF EXISTS chk_deploy_deployment_environment;
ALTER TABLE deploy_deployment
    ADD CONSTRAINT chk_deploy_deployment_environment CHECK (
        environment IN ('development', 'test', 'staging', 'demo', 'production')
    );

-- 3. Platform vocabulary on the target row. The kind -> platform pair stays a
--    service rule (`sdkwork-deploy-core::validate_app_kind_platform`); this
--    CHECK only refuses a platform value the matrix does not contain at all.

UPDATE deploy_app_platform_target
   SET platform = UPPER(TRIM(platform)),
       updated_at = NOW(),
       version = version + 1
 WHERE platform <> UPPER(TRIM(platform));

DO $$
DECLARE
    offending TEXT;
BEGIN
    SELECT STRING_AGG(
               FORMAT('app_id=%s platform=%L', app_id, platform),
               '; ' ORDER BY app_id, platform)
      INTO offending
      FROM deploy_app_platform_target
     WHERE platform NOT IN (
             'WEB', 'API', 'WECHAT', 'DOUYIN', 'IOS', 'ANDROID',
             'HARMONYOS', 'WINDOWS', 'MACOS', 'LINUX'
         );

    IF offending IS NOT NULL THEN
        RAISE EXCEPTION
            '0015 cannot constrain deploy_app_platform_target.platform: % row(s) carry a platform outside the endpoint matrix; fix them before applying this migration',
            offending
            USING ERRCODE = 'check_violation';
    END IF;
END $$;

ALTER TABLE deploy_app_platform_target
    DROP CONSTRAINT IF EXISTS chk_deploy_app_platform_target_platform;
ALTER TABLE deploy_app_platform_target
    ADD CONSTRAINT chk_deploy_app_platform_target_platform CHECK (
        platform IN (
            'WEB', 'API', 'WECHAT', 'DOUYIN', 'IOS', 'ANDROID',
            'HARMONYOS', 'WINDOWS', 'MACOS', 'LINUX'
        )
    );

COMMIT;
