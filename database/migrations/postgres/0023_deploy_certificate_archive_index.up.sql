-- sdkwork:migration
-- id: 0023_deploy_certificate_archive_index
-- engine: postgres
-- module: deploy
-- purpose: Widen the archive due-index so a retried (FAILED) row is also claimable, and
--   add the read-model join index.
--
--   `0019_deploy_certificate_archive` created
--     idx_deploy_certificate_archive_due (status, next_attempt_at, id)
--     WHERE status IN ('PENDING', 'STORING')
--   while the retry policy in `sdkwork-intelligence-deploy-service::certificate_archive`
--   moves a row to FAILED once the bounded attempt budget is spent, and the console's
--   "retry" command moves it back to PENDING. Between those two instants a row that an
--   operator has just retried is not distinguishable from a fresh one, so the index has
--   to cover exactly what the claim query reads: every row that still has an attempt
--   left, due now or later. STORING is included because a worker that died mid-upload
--   leaves its row there and the next pass has to reclaim it.
--
--   The partial index is not a superset of the old one (it drops STORING-only rows that
--   the old predicate admitted and FAILED rows it did not), so it is replaced rather
--   than added: two overlapping partial indexes would make the planner pick one and
--   scan it for the other's predicate.
--
--   `idx_deploy_certificate_archive_due` is empty on every deployment that has never
--   archived a certificate, so the rebuild is cheap; `CONCURRENTLY` is deliberately not
--   used because it cannot run inside the transactional migration envelope and a
--   concurrent build that fails leaves an INVALID index behind that a migration runner
--   would then report as drift forever.
--
-- reversible: true
-- rollback: 0023_deploy_certificate_archive_index.down.sql restores the original predicate
-- transactional: true
-- lock: exclusive (share) on deploy_certificate_archive; index-only rebuild, no table scan
-- lock_timeout: 2s
-- statement_timeout: 30s

DROP INDEX IF EXISTS idx_deploy_certificate_archive_due;

CREATE INDEX IF NOT EXISTS idx_deploy_certificate_archive_due
    ON deploy_certificate_archive (status, next_attempt_at, id)
    WHERE status IN ('PENDING', 'STORING', 'FAILED');

-- The certificate read model resolves "the archive row of the version being served",
-- by (version, kind), once per list row. The unique constraint already indexes that
-- pair, so this only adds the covering read the console needs without depending on
-- the constraint's own ordering.
CREATE INDEX IF NOT EXISTS idx_deploy_certificate_archive_current_version
    ON deploy_certificate_archive (tenant_id, certificate_version_id, bundle_kind);
