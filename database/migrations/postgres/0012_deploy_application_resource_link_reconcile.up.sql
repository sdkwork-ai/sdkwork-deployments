-- sdkwork:migration
-- id: 0012_deploy_application_resource_link_reconcile
-- engine: postgres
-- module: sdkwork-deploy
-- purpose: Make `deploy_application_resource_link` trustworthy before any code
--   writes it. The table shipped in the baseline with three defects that only
--   matter once it is the authority for account bindings:
--
--   1. `env_key` had no lifecycle-environment CHECK, unlike every other
--      environment-scoped table (`chk_deploy_app_binding_environment`, …). A typo
--      like `prod` would sit in the row forever, invisible to every per-environment
--      query.
--   2. `uk_deploy_application_resource_link_default` keyed on `(app_id, env_key,
--      provider_code)` with `provider_code` nullable. PostgreSQL treats NULLs as
--      distinct in a unique index, so two `is_default` payment rows with a NULL
--      provider both satisfied the index: the "one default per provider" rule was
--      unenforced exactly where it mattered.
--   3. `resource_ref` accepted an empty string and `provider_code` an empty string,
--      so a row could "bind" an account whose provider-side identity is blank.
--
--   The identifiers are deliberately NOT converted to the module's
--   `BIGINT id + uuid` pair in this migration. `DATABASE_SPEC.md` §6.1 forbids a
--   migration from allocating ids ad hoc (`MAX(id)+1`, sequences, random hashes),
--   and §6.3 requires a backfill to preserve stable ids or use a documented
--   deterministic remap. This table's `id` already *is* a stable public id (a
--   uuid string), nothing references it through a foreign key, and the API exposes
--   it as `id`; introducing a second identifier now would buy no referential
--   power and would require the backfill in 0013 to mint ids in SQL. The
--   condition for revisiting is documented in
--   docs/architecture/tech/TECH-application-resource-bindings.md: the first
--   external table that needs a foreign key into a binding.
-- reversible: true
-- rollback: down-migration (drops the four CHECK constraints added here and
--   restores the previous `uk_deploy_application_resource_link_default` definition)
-- transactional: true
-- lock: ACCESS EXCLUSIVE on deploy_application_resource_link (constraint and index
--   rebuild); the table has no readers and no writers outside this change
-- lock_timeout: 2s
-- statement_timeout: 60s

BEGIN;

-- The table was folded into the baseline after schemas were already baselined,
-- so an already-initialized database does not have it. Create it verbatim from
-- `database/ddl/baseline/postgres/0001_deploy_baseline.sql` before reconciling;
-- fresh installs already created it and this is a no-op there.
CREATE TABLE IF NOT EXISTS deploy_application_resource_link (
    id              VARCHAR(36)  NOT NULL,
    tenant_id       BIGINT       NOT NULL,
    organization_id BIGINT       NOT NULL DEFAULT 0,
    app_id          BIGINT       NOT NULL,
    env_key         VARCHAR(32)  NOT NULL,
    resource_kind   VARCHAR(24)  NOT NULL,
    resource_ref    VARCHAR(128) NOT NULL,
    provider_code   VARCHAR(32),
    is_default      BOOLEAN      NOT NULL DEFAULT FALSE,
    binding_status  VARCHAR(16)  NOT NULL DEFAULT 'ACTIVE',
    metadata        JSONB        NOT NULL DEFAULT '{}',
    created_by      BIGINT,
    updated_by      BIGINT,
    created_at      TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    deleted_at      TIMESTAMPTZ,
    version         BIGINT       NOT NULL DEFAULT 1,
    CONSTRAINT pk_deploy_application_resource_link PRIMARY KEY (id),
    CONSTRAINT fk_deploy_application_resource_link_app
        FOREIGN KEY (app_id) REFERENCES deploy_app(id),
    CONSTRAINT chk_deploy_application_resource_link_kind
        CHECK (resource_kind IN ('official_account', 'mini_program', 'payment_account')),
    CONSTRAINT chk_deploy_application_resource_link_status
        CHECK (binding_status IN ('ACTIVE', 'DISABLED'))
);

CREATE UNIQUE INDEX IF NOT EXISTS uk_deploy_application_resource_link_ref
    ON deploy_application_resource_link (app_id, env_key, resource_kind, resource_ref)
    WHERE deleted_at IS NULL;

CREATE UNIQUE INDEX IF NOT EXISTS uk_deploy_application_resource_link_single
    ON deploy_application_resource_link (app_id, env_key, resource_kind)
    WHERE resource_kind IN ('official_account', 'mini_program') AND deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_deploy_application_resource_link_app
    ON deploy_application_resource_link (tenant_id, app_id, env_key, resource_kind, deleted_at, id);

-- A row written before this migration cannot be repaired by guesswork: the table
-- had no writer, so any row in it was hand-written and its intent is unknown.
-- Refuse loudly instead of silently coercing data, and say what to do. This is a
-- no-op on every deployment that only ran the baseline.
DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM deploy_application_resource_link
        WHERE env_key NOT IN ('development', 'test', 'staging', 'demo', 'production')
           OR btrim(resource_ref) = ''
           OR char_length(resource_ref) > 128
           OR (provider_code IS NOT NULL AND btrim(provider_code) = '')
           OR (provider_code IS NOT NULL AND char_length(provider_code) > 32)
           OR (resource_kind = 'payment_account' AND provider_code IS NULL)
           OR (is_default AND resource_kind <> 'payment_account')
    ) THEN
        RAISE EXCEPTION
            'deploy_application_resource_link holds rows that violate the reconciled constraints; '
            'the table had no writer, so repair or delete them by hand before applying 0012 '
            '(see docs/architecture/tech/TECH-application-resource-bindings.md)';
    END IF;
END
$$;

-- 1. Lifecycle environment, matching the five values every other
--    environment-scoped deploy table accepts (`chk_deploy_app_binding_environment`,
--    `chk_deploy_app_resource_environment`, …).
ALTER TABLE deploy_application_resource_link
    DROP CONSTRAINT IF EXISTS chk_deploy_application_resource_link_environment;
ALTER TABLE deploy_application_resource_link
    ADD CONSTRAINT chk_deploy_application_resource_link_environment
    CHECK (env_key IN ('development', 'test', 'staging', 'demo', 'production'));

-- 2. A binding must name something. The reference is the provider-side identity
--    (WeChat AppID, merchant account number); blank is not "unset", it is a row
--    the runtime can never resolve.
ALTER TABLE deploy_application_resource_link
    DROP CONSTRAINT IF EXISTS chk_deploy_application_resource_link_ref;
ALTER TABLE deploy_application_resource_link
    ADD CONSTRAINT chk_deploy_application_resource_link_ref
    CHECK (btrim(resource_ref) <> '' AND char_length(resource_ref) <= 128);

-- 3. Provider. Required for a payment account (its default uniqueness is keyed by
--    provider), optional for a channel account (the provider is derivable from the
--    kind, and legacy rows carry none), never blank when present.
ALTER TABLE deploy_application_resource_link
    DROP CONSTRAINT IF EXISTS chk_deploy_application_resource_link_provider;
ALTER TABLE deploy_application_resource_link
    ADD CONSTRAINT chk_deploy_application_resource_link_provider
    CHECK (
        (provider_code IS NULL OR (btrim(provider_code) <> '' AND char_length(provider_code) <= 32))
        AND (resource_kind <> 'payment_account' OR provider_code IS NOT NULL)
    );

-- 4. `is_default` is a payment concept. An official account or mini program is
--    single-instance, so it *is* the environment's account and has no default to
--    elect; allowing the flag there would let two rows disagree about a fact that
--    `uk_single` already decides.
ALTER TABLE deploy_application_resource_link
    DROP CONSTRAINT IF EXISTS chk_deploy_application_resource_link_default_kind;
ALTER TABLE deploy_application_resource_link
    ADD CONSTRAINT chk_deploy_application_resource_link_default_kind
    CHECK (NOT is_default OR resource_kind = 'payment_account');

-- 5. The default slot. `COALESCE(provider_code, '')` closes the NULL hole: even if
--    constraint 3 is ever dropped, two defaults for the same provider can no
--    longer coexist. The predicate keeps the index proportional to the default
--    set (at most one row per provider per environment) instead of the whole
--    table.
DROP INDEX IF EXISTS uk_deploy_application_resource_link_default;
CREATE UNIQUE INDEX IF NOT EXISTS uk_deploy_application_resource_link_default
    ON deploy_application_resource_link (app_id, env_key, COALESCE(provider_code, ''))
    WHERE is_default AND resource_kind = 'payment_account' AND deleted_at IS NULL;

COMMENT ON COLUMN deploy_application_resource_link.id IS
    '绑定行的公开标识（uuid 字符串）；本表不被任何外键引用，转换 BIGINT id + uuid 的时机见 0012 迁移头与 TECH-application-resource-bindings.md';
COMMENT ON COLUMN deploy_application_resource_link.env_key IS
    '生命周期环境：development | test | staging | demo | production；由 chk_deploy_application_resource_link_environment 约束';
COMMENT ON COLUMN deploy_application_resource_link.resource_kind IS
    '账号类型：official_account（公众号）| mini_program（小程序）| payment_account（支付账号）';
COMMENT ON COLUMN deploy_application_resource_link.resource_ref IS
    '提供方侧标识：公众号/小程序的 AppID，支付账号的商户账号（充值/结算账户号）';
COMMENT ON COLUMN deploy_application_resource_link.provider_code IS
    '账号所属提供方（wechat、alipay、stripe 等）；支付账号必填，因为默认账号按提供方唯一';
COMMENT ON COLUMN deploy_application_resource_link.is_default IS
    '是否为该环境下该提供方的默认支付账号；公众号/小程序为单实例，恒为 FALSE';
COMMENT ON COLUMN deploy_application_resource_link.metadata IS
    '绑定当时批准的展示快照与提供方事实（displayName、accountMode、结算币种等）';
COMMENT ON TABLE deploy_application_resource_link IS
    '部署应用在某环境下的账号绑定（公众号/小程序/支付账号）；账号绑定的权威表，deploy_app.metadata.integrations 仅为其历史输入';

COMMIT;
