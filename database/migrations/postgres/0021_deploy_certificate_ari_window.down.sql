-- sdkwork:migration
-- id: 0021_deploy_certificate_ari_window
-- engine: postgres
-- module: deploy
-- purpose: Restore the pre-ARI certificate shape.
-- reversible: true
-- rollback: forward-migration
-- transactional: true
-- lock: ACCESS EXCLUSIVE on deploy_certificate
-- lock_timeout: 2s
-- statement_timeout: 30s
--
-- Dropping these columns only discards the CA's *suggestion*; the local renewal
-- window takes over again, and no certificate, version or order is affected.

ALTER TABLE deploy_certificate
    DROP CONSTRAINT IF EXISTS chk_deploy_certificate_ari_window;

ALTER TABLE deploy_certificate
    DROP COLUMN IF EXISTS ari_checked_at,
    DROP COLUMN IF EXISTS ari_window_end,
    DROP COLUMN IF EXISTS ari_window_start;
