-- sdkwork:migration
-- id: 0008_deploy_app_identity_provisioning
-- engine: postgres
-- module: sdkwork-deploy
-- purpose: The deployment application becomes the single application authority
--   (APPLICATION_FACTORY_READINESS_GAP_REPORT.md, R1/R7). Creating a deployment
--   application must provision its IAM installation, and that cross-module step
--   needs a state the deploy module owns:
--
--   - identity_status: pending_identity | active | failed, so an application
--     whose installation was never provisioned is visible instead of silently
--     unusable.
--   - identity_provisioning_key: the idempotency key sent to IAM, so retrying
--     the call cannot create a second installation.
--
--   Additive: existing rows start as pending_identity and are flipped by the
--   backfill once their installation is linked.
-- reversible: true
-- rollback: down-migration (the paired down migration drops the columns and
--   indexes)
-- transactional: true
-- lock: ACCESS EXCLUSIVE on deploy_app for the added columns
-- lock_timeout: 2s
-- statement_timeout: 60s

BEGIN;

ALTER TABLE deploy_app
    ADD COLUMN IF NOT EXISTS identity_status TEXT NOT NULL DEFAULT 'pending_identity',
    ADD COLUMN IF NOT EXISTS identity_provisioning_key TEXT;

ALTER TABLE deploy_app
    ADD CONSTRAINT chk_deploy_app_identity_status
    CHECK (identity_status IN ('pending_identity', 'active', 'failed'));

CREATE UNIQUE INDEX IF NOT EXISTS uk_deploy_app_identity_provisioning_key
    ON deploy_app (identity_provisioning_key)
    WHERE identity_provisioning_key IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_deploy_app_identity_status
    ON deploy_app (tenant_id, identity_status, updated_at, id);

COMMIT;
