-- sdkwork:migration
-- id: 0023_deploy_certificate_archive_index
-- engine: postgres
-- module: deploy
-- purpose: Restore the archive due-index predicate `0019` created.
--
--   Safe to run before the service is rolled back: the narrow predicate is a subset of
--   what the widened one covered, and the column set is unchanged, so the previous
--   claim query keeps using it.

DROP INDEX IF EXISTS idx_deploy_certificate_archive_current_version;
DROP INDEX IF EXISTS idx_deploy_certificate_archive_due;

CREATE INDEX IF NOT EXISTS idx_deploy_certificate_archive_due
    ON deploy_certificate_archive (status, next_attempt_at, id)
    WHERE status IN ('PENDING', 'STORING');
