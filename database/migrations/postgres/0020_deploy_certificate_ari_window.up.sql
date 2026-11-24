-- sdkwork:migration
-- id: 0020_deploy_certificate_ari_window
-- engine: postgres
-- module: deploy
-- purpose: Record the CA's suggested renewal window (RFC 9773 ARI) on the certificate.
--
--   The local rule (`max(renewBeforeDays, lifetime/3)`) is the contractual
--   promise, but a CA can ask for earlier renewal when it plans a revocation or a
--   CA-wide change. Storing the suggestion lets the renewal scheduler take
--   `min(ARI, local)` without asking the CA on every tick.
--
-- reversible: true
-- rollback: 0020_deploy_certificate_ari_window.down.sql
-- transactional: true
-- lock: ACCESS EXCLUSIVE on deploy_certificate
-- lock_timeout: 2s
-- statement_timeout: 30s

ALTER TABLE deploy_certificate
    ADD COLUMN IF NOT EXISTS ari_window_start TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS ari_window_end   TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS ari_checked_at   TIMESTAMPTZ;

DO $$
BEGIN
    -- A window is either fully recorded (start, end and the instant it was
    -- observed) or absent. A half-written window would let the scheduler adopt a
    -- bound nobody measured, and an unobserved one could not be audited.
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'chk_deploy_certificate_ari_window') THEN
        ALTER TABLE deploy_certificate
            ADD CONSTRAINT chk_deploy_certificate_ari_window
            CHECK (
                (ari_window_start IS NULL AND ari_window_end IS NULL AND ari_checked_at IS NULL)
                OR (
                    ari_window_start IS NOT NULL
                    AND ari_window_end IS NOT NULL
                    AND ari_checked_at IS NOT NULL
                    AND ari_window_start < ari_window_end
                )
            );
    END IF;
END $$;
