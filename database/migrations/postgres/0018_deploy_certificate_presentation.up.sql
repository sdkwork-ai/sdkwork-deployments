-- sdkwork:migration
-- id: 0018_deploy_certificate_presentation
-- engine: postgres
-- module: deploy
-- purpose: Let a certificate order wait for an operator to publish its DNS-01 record.
--
--   Until now every order was driven to completion by one worker call: the CA
--   order was created, the challenge was presented by the platform, and the CA
--   was asked to validate, all inside a single function. A tenant whose DNS zone
--   has no integrated provider account therefore could not issue at all.
--
--   `presentation_mode` records who presents the challenge (`MANAGED` = the
--   platform, `MANUAL` = an operator). `external_order_url` persists the CA order
--   identity, which is the only state needed to resume an order
--   (`instant_acme::Account::order(url)` re-fetches it). `presentation_ready_at`
--   is the operator's explicit release: an order with `MANUAL` and a NULL value
--   is parked, and the issuance worker must not claim it.
--
-- reversible: true
-- rollback: 0018_deploy_certificate_presentation.down.sql
-- transactional: true
-- lock: ACCESS EXCLUSIVE on deploy_certificate_order and deploy_certificate
-- lock_timeout: 2s
-- statement_timeout: 30s

ALTER TABLE deploy_certificate_order
    ADD COLUMN IF NOT EXISTS presentation_mode VARCHAR(16) NOT NULL DEFAULT 'MANAGED',
    ADD COLUMN IF NOT EXISTS external_order_url VARCHAR(1024),
    ADD COLUMN IF NOT EXISTS presentation_ready_at TIMESTAMPTZ;

ALTER TABLE deploy_certificate
    ADD COLUMN IF NOT EXISTS presentation_mode VARCHAR(16) NOT NULL DEFAULT 'MANAGED';

DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'chk_deploy_certificate_order_presentation_mode') THEN
        ALTER TABLE deploy_certificate_order
            ADD CONSTRAINT chk_deploy_certificate_order_presentation_mode
            CHECK (presentation_mode IN ('MANAGED', 'MANUAL'));
    END IF;
    -- Only an https URL may be resumed: the order URL is fetched with the
    -- account key, so a non-TLS origin is never acceptable.
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'chk_deploy_certificate_order_external_order_url') THEN
        ALTER TABLE deploy_certificate_order
            ADD CONSTRAINT chk_deploy_certificate_order_external_order_url
            CHECK (external_order_url IS NULL OR external_order_url ~ '^https://');
    END IF;
    -- A MANAGED order never needs the operator release, and a MANUAL order is
    -- the only one that may carry it. Keeping the pair consistent in the
    -- database is what stops a released flag from surviving a mode change.
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'chk_deploy_certificate_order_presentation_ready') THEN
        ALTER TABLE deploy_certificate_order
            ADD CONSTRAINT chk_deploy_certificate_order_presentation_ready
            CHECK (presentation_mode = 'MANUAL' OR presentation_ready_at IS NULL);
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'chk_deploy_certificate_presentation_mode') THEN
        ALTER TABLE deploy_certificate
            ADD CONSTRAINT chk_deploy_certificate_presentation_mode
            CHECK (presentation_mode IN ('MANAGED', 'MANUAL'));
    END IF;
END $$;

-- The issuance worker claims due orders; a parked manual order must not be
-- retried every tick, so the claim query filters on this pair.
CREATE INDEX IF NOT EXISTS idx_deploy_certificate_order_presentation
    ON deploy_certificate_order (presentation_mode, presentation_ready_at, next_attempt_at)
    WHERE status NOT IN ('VERSION_STORED', 'FAILED', 'CANCELLED');
