-- sdkwork:migration
-- id: 0016_environment_promotion_approver
-- engine: postgres
-- module: deploy
-- purpose: Record who approved an approval-gated promotion, so "approval_required"
--   is a separation-of-duties control instead of a caller-supplied boolean.
--
--   Before this column, `deploy_app_environment.approval_required` could be
--   satisfied by the same caller sending `approve: true`: the flag documented an
--   intent, but nothing recorded a second person and nothing distinguished the
--   promoter from an approver. The promotion row now carries the approver's user
--   subject, the service refuses an approving subject that equals the promoter,
--   and the history keeps the pair for audit.
--
--   Nullable on purpose: every promotion that predates this migration, and every
--   promotion into an environment that does not require approval, has no approver
--   and must stay readable.
-- reversible: false
-- rollback: forward-fix (dropping the column would lose the approval evidence
--   this migration exists to record; DATABASE_FRAMEWORK_SPEC.md section 7.1
--   permits a down file only for a bounded, data-preserving reversal)
-- transactional: true
-- lock: ACCESS EXCLUSIVE on deploy_environment_promotion for the added column
-- lock_timeout: 2s
-- statement_timeout: 30s

BEGIN;

ALTER TABLE deploy_environment_promotion
    ADD COLUMN IF NOT EXISTS approver_user_id BIGINT;

COMMENT ON COLUMN deploy_environment_promotion.approver_user_id IS
    'User subject that approved an approval-gated promotion. MUST differ from promoted_by; NULL for promotions into environments that do not require approval and for rows that predate the column.';

CREATE INDEX IF NOT EXISTS idx_deploy_environment_promotion_approver
    ON deploy_environment_promotion (tenant_id, approver_user_id)
    WHERE approver_user_id IS NOT NULL;

COMMIT;
