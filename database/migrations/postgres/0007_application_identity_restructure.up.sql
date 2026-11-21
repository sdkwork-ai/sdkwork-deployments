-- sdkwork:migration
-- id: 0007_application_identity_restructure
-- engine: postgres
-- module: sdkwork-deployments
-- purpose: First phase of the application-factory restructure
--   (APPLICATION_FACTORY_DB_RESTRUCTURE_PLAN.md).
--
--   The baseline now declares the end state directly: `deploy_application_scaffold`
--   (+ `_category`, `_version`) replaces the storefront template family, and
--   `deploy_build_recipe` replaces `deploy_build_template`. Because the baseline is
--   also the contract source for this module, the rename must live there rather
--   than in a forward `RENAME` migration.
--
--   This migration therefore converges already-initialized schemas: it creates
--   the end-state tables the baseline only creates for a fresh install, re-points
--   the two child foreign keys that still reference `deploy_build_template`, and
--   only then removes the pre-restructure tables the historical replay migrations
--   (0001-0006) created. Nothing was ever released, so those tables carry no
--   production rows.
-- reversible: false
-- rollback: forward-fix (the legacy tables are removed; a database that still
--   needs their rows must be exported before this migration runs, so no down
--   migration is offered)
-- transactional: true
-- lock: ACCESS EXCLUSIVE on the dropped legacy tables
-- lock_timeout: 2s
-- statement_timeout: 60s

BEGIN;

-- End-state tables, mirroring database/ddl/baseline/postgres/0001_deploy_baseline.sql
-- verbatim so a schema baselined before they existed converges without a reset.

CREATE TABLE IF NOT EXISTS deploy_build_recipe (
    id              BIGINT       NOT NULL,
    uuid            VARCHAR(36)  NOT NULL,
    tenant_id       BIGINT       NOT NULL,
    organization_id BIGINT       NOT NULL DEFAULT 0,
    template_name   VARCHAR(200) NOT NULL,
    template_version VARCHAR(64) NOT NULL,
    platform        VARCHAR(16)  NOT NULL,
    tech_stack      VARCHAR(16)  NOT NULL DEFAULT 'OTHER',
    toolchain_json  JSONB        NOT NULL DEFAULT '{}',
    commands_json   JSONB        NOT NULL DEFAULT '[]',
    artifact_outputs_json JSONB  NOT NULL DEFAULT '[]',
    quality_gates_json JSONB    NOT NULL DEFAULT '{}',
    template_status VARCHAR(16)  NOT NULL DEFAULT 'DRAFT',
    created_by      BIGINT,
    updated_by      BIGINT,
    created_at      TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    deleted_at      TIMESTAMPTZ,
    version         BIGINT       NOT NULL DEFAULT 1,
    CONSTRAINT pk_deploy_build_recipe PRIMARY KEY (id)
);

CREATE UNIQUE INDEX IF NOT EXISTS uk_deploy_build_recipe_uuid
    ON deploy_build_recipe (uuid);

CREATE UNIQUE INDEX IF NOT EXISTS uk_deploy_build_recipe_name_version
    ON deploy_build_recipe (tenant_id, template_name, template_version)
    WHERE deleted_at IS NULL;

-- 依赖顺序：分类先于模板，模板先于版本（同脚本内 FK 需要目标先建）。

CREATE TABLE IF NOT EXISTS deploy_application_scaffold_category (
    id              BIGINT        NOT NULL,
    uuid            VARCHAR(36)   NOT NULL,
    tenant_id       BIGINT        NOT NULL,
    organization_id BIGINT        NOT NULL DEFAULT 0,
    parent_id       BIGINT,
    category_key    VARCHAR(64)   NOT NULL,
    display_name    VARCHAR(128)  NOT NULL,
    description     VARCHAR(512),
    sort_order      INTEGER       NOT NULL DEFAULT 0,
    status          VARCHAR(16)   NOT NULL DEFAULT 'ACTIVE',
    created_by      BIGINT,
    updated_by      BIGINT,
    created_at      TIMESTAMPTZ   NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ   NOT NULL DEFAULT NOW(),
    version         BIGINT        NOT NULL DEFAULT 1,
    deleted_at      TIMESTAMPTZ,
    CONSTRAINT pk_deploy_application_scaffold_category PRIMARY KEY (id),
    CONSTRAINT uk_deploy_application_scaffold_category_uuid UNIQUE (uuid),
    CONSTRAINT fk_deploy_application_scaffold_category_parent FOREIGN KEY (parent_id) REFERENCES deploy_application_scaffold_category(id),
    CONSTRAINT chk_deploy_application_scaffold_category_status CHECK (status IN ('ACTIVE', 'DISABLED'))
);

CREATE UNIQUE INDEX IF NOT EXISTS uk_deploy_application_scaffold_category_tenant_key
    ON deploy_application_scaffold_category (tenant_id, category_key)
    WHERE deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_deploy_application_scaffold_category_browse
    ON deploy_application_scaffold_category (tenant_id, parent_id, status, sort_order, id)
    WHERE deleted_at IS NULL;

CREATE TABLE IF NOT EXISTS deploy_application_scaffold (
    id               BIGINT        NOT NULL,
    uuid             VARCHAR(36)   NOT NULL,
    tenant_id        BIGINT        NOT NULL,
    organization_id  BIGINT        NOT NULL DEFAULT 0,
    category_id      BIGINT        NOT NULL,
    author_user_id   BIGINT        NOT NULL,
    app_uuid         VARCHAR(36)   NOT NULL,
    template_type    VARCHAR(16)   NOT NULL DEFAULT 'APP',
    template_key     VARCHAR(64)   NOT NULL,
    display_name     VARCHAR(200)  NOT NULL,
    summary          VARCHAR(512)  NOT NULL,
    description      TEXT          NOT NULL DEFAULT '',
    icon_media_ref   VARCHAR(512),
    cover_media_ref  VARCHAR(512),
    visibility       VARCHAR(16)   NOT NULL DEFAULT 'PRIVATE',
    pricing_model    VARCHAR(16)   NOT NULL DEFAULT 'FREE',
    price_minor      BIGINT        NOT NULL DEFAULT 0,
    currency         VARCHAR(8)    NOT NULL DEFAULT 'CNY',
    status           VARCHAR(16)   NOT NULL DEFAULT 'DRAFT',
    review_note      VARCHAR(512),
    is_featured      BOOLEAN       NOT NULL DEFAULT FALSE,
    install_count    BIGINT        NOT NULL DEFAULT 0,
    view_count       BIGINT        NOT NULL DEFAULT 0,
    latest_version_uuid VARCHAR(36),
    metadata         JSONB         NOT NULL DEFAULT '{}',
    created_by       BIGINT,
    updated_by       BIGINT,
    created_at       TIMESTAMPTZ   NOT NULL DEFAULT NOW(),
    updated_at       TIMESTAMPTZ   NOT NULL DEFAULT NOW(),
    version          BIGINT        NOT NULL DEFAULT 1,
    deleted_at       TIMESTAMPTZ,
    app_config_schema JSONB NOT NULL DEFAULT '{}',
    variable_schema JSONB NOT NULL DEFAULT '{}',
    dependency_manifest JSONB NOT NULL DEFAULT '{}',
    capability_manifest JSONB NOT NULL DEFAULT '{}',
    license_code VARCHAR(64),
    CONSTRAINT pk_deploy_application_scaffold PRIMARY KEY (id),
    CONSTRAINT uk_deploy_application_scaffold_uuid UNIQUE (uuid),
    CONSTRAINT fk_deploy_application_scaffold_category FOREIGN KEY (category_id) REFERENCES deploy_application_scaffold_category(id),
    CONSTRAINT chk_deploy_application_scaffold_type CHECK (template_type IN ('APP', 'PPT', 'VIDEO')),
    CONSTRAINT chk_deploy_application_scaffold_visibility CHECK (visibility IN ('PUBLIC', 'PRIVATE')),
    CONSTRAINT chk_deploy_application_scaffold_pricing CHECK (pricing_model IN ('FREE', 'PAID')),
    CONSTRAINT chk_deploy_application_scaffold_price CHECK (
        (pricing_model = 'PAID' AND price_minor > 0) OR (pricing_model = 'FREE' AND price_minor = 0)
    ),
    CONSTRAINT chk_deploy_application_scaffold_status CHECK (
        status IN ('DRAFT', 'PENDING_REVIEW', 'PUBLISHED', 'REJECTED', 'DISABLED')
    )
);

CREATE UNIQUE INDEX IF NOT EXISTS uk_deploy_application_scaffold_tenant_key
    ON deploy_application_scaffold (tenant_id, template_key)
    WHERE deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_deploy_application_scaffold_marketplace
    ON deploy_application_scaffold (tenant_id, status, visibility, category_id, updated_at DESC, id)
    WHERE deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_deploy_application_scaffold_marketplace_type
    ON deploy_application_scaffold (tenant_id, status, visibility, template_type, updated_at DESC, id)
    WHERE deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_deploy_application_scaffold_author
    ON deploy_application_scaffold (tenant_id, author_user_id, status, updated_at DESC, id)
    WHERE deleted_at IS NULL;

CREATE TABLE IF NOT EXISTS deploy_application_scaffold_version (
    id                  BIGINT        NOT NULL,
    uuid                VARCHAR(36)   NOT NULL,
    tenant_id           BIGINT        NOT NULL,
    organization_id     BIGINT        NOT NULL DEFAULT 0,
    template_id         BIGINT        NOT NULL,
    template_version    VARCHAR(64)   NOT NULL,
    changelog           TEXT          NOT NULL DEFAULT '',
    artifact_uuid       VARCHAR(36),
    source_app_version  VARCHAR(64),
    platform_targets_json JSONB      NOT NULL DEFAULT '[]',
    package_size_bytes  BIGINT        NOT NULL DEFAULT 0,
    checksum_sha256     VARCHAR(128),
    status              VARCHAR(16)   NOT NULL DEFAULT 'DRAFT',
    published_at        TIMESTAMPTZ,
    metadata            JSONB         NOT NULL DEFAULT '{}',
    created_by          BIGINT,
    updated_by          BIGINT,
    created_at          TIMESTAMPTZ   NOT NULL DEFAULT NOW(),
    updated_at          TIMESTAMPTZ   NOT NULL DEFAULT NOW(),
    version             BIGINT        NOT NULL DEFAULT 1,
    deleted_at          TIMESTAMPTZ,
    CONSTRAINT pk_deploy_application_scaffold_version PRIMARY KEY (id),
    CONSTRAINT uk_deploy_application_scaffold_version_uuid UNIQUE (uuid),
    CONSTRAINT fk_deploy_application_scaffold_version_template FOREIGN KEY (template_id) REFERENCES deploy_application_scaffold(id),
    CONSTRAINT chk_deploy_application_scaffold_version_status CHECK (status IN ('DRAFT', 'PUBLISHED', 'WITHDRAWN'))
);

CREATE UNIQUE INDEX IF NOT EXISTS uk_deploy_application_scaffold_version_number
    ON deploy_application_scaffold_version (template_id, template_version)
    WHERE deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_deploy_application_scaffold_version_list
    ON deploy_application_scaffold_version (tenant_id, template_id, status, created_at DESC, id)
    WHERE deleted_at IS NULL;

COMMENT ON TABLE deploy_application_scaffold_category IS '应用模板市场分类；平台管理员维护，浏览树最多两层';
COMMENT ON COLUMN deploy_application_scaffold_category.category_key IS '租户内稳定的分类标识，模板引用与统计都以它为准';
COMMENT ON TABLE deploy_application_scaffold IS '应用模板：作者发布的可获取应用形态，含可见性、定价与审核状态';
COMMENT ON COLUMN deploy_application_scaffold.app_uuid IS '来源 deploy_app.uuid，引用不级联：模板独立于应用行存在';
COMMENT ON COLUMN deploy_application_scaffold.visibility IS 'PUBLIC 进市场浏览；PRIVATE 仅作者与自己租户可见';
COMMENT ON COLUMN deploy_application_scaffold.pricing_model IS 'FREE 零元直接获取；PAID 模板经 sdkwork-order 下单、sdkwork-payment 支付，权益由订单中心持有';
COMMENT ON COLUMN deploy_application_scaffold.price_minor IS '标价，货币最小单位（分）；FREE 恒为 0';
COMMENT ON COLUMN deploy_application_scaffold.status IS 'DRAFT→PENDING_REVIEW→PUBLISHED；REJECTED 退回作者；DISABLED 平台下架';
COMMENT ON TABLE deploy_application_scaffold_version IS '应用模板版本：一次发布的不可变快照，含产物引用与校验和';
COMMENT ON COLUMN deploy_application_scaffold_version.artifact_uuid IS '打包产物 deploy_artifact.uuid；未打包为 NULL';

-- Child foreign keys must follow the replacement table before the legacy table
-- can be dropped. The new baseline already declares these constraint names
-- against `deploy_build_recipe`, so drop-and-recreate is idempotent for fresh
-- installs and re-points already-initialized schemas.
ALTER TABLE deploy_app_platform_target
    DROP CONSTRAINT IF EXISTS fk_deploy_app_platform_target_template;
ALTER TABLE deploy_app_platform_target
    ADD CONSTRAINT fk_deploy_app_platform_target_template
    FOREIGN KEY (build_template_id) REFERENCES deploy_build_recipe(id);

ALTER TABLE deploy_build
    DROP CONSTRAINT IF EXISTS fk_deploy_build_template;
ALTER TABLE deploy_build
    ADD CONSTRAINT fk_deploy_build_template
    FOREIGN KEY (template_id) REFERENCES deploy_build_recipe(id);

-- Order matters: the version table references the template table.
DROP TABLE IF EXISTS deploy_app_template_version;
DROP TABLE IF EXISTS deploy_app_template;
DROP TABLE IF EXISTS deploy_app_template_category;
DROP TABLE IF EXISTS deploy_build_template;

COMMIT;
