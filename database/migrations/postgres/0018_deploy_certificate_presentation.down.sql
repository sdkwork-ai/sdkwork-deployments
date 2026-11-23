-- sdkwork:migration
-- id: 0018_deploy_certificate_presentation
-- engine: postgres
-- module: deploy
-- purpose: Restore the pre-presentation order shape.
--
-- reversible: true
-- rollback: forward-migration
-- transactional: true
-- lock: ACCESS EXCLUSIVE on deploy_certificate_order and deploy_certificate
-- lock_timeout: 2s
-- statement_timeout: 30s
--
-- Dropping these columns discards the CA order identity of any in-flight manual
-- order; such an order can no longer be resumed and must be re-requested. The
-- certificate row itself and every stored version are untouched.

DROP INDEX IF EXISTS idx_deploy_certificate_order_presentation;

ALTER TABLE deploy_certificate_order
    DROP CONSTRAINT IF EXISTS chk_deploy_certificate_order_presentation_ready,
    DROP CONSTRAINT IF EXISTS chk_deploy_certificate_order_external_order_url,
    DROP CONSTRAINT IF EXISTS chk_deploy_certificate_order_presentation_mode;

ALTER TABLE deploy_certificate
    DROP CONSTRAINT IF EXISTS chk_deploy_certificate_presentation_mode;

ALTER TABLE deploy_certificate_order
    DROP COLUMN IF EXISTS presentation_ready_at,
    DROP COLUMN IF EXISTS external_order_url,
    DROP COLUMN IF EXISTS presentation_mode;

ALTER TABLE deploy_certificate
    DROP COLUMN IF EXISTS presentation_mode;
