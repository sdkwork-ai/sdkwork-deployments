-- sdkwork:migration
-- id: 0001_deploy_app_template_marketplace
-- engine: postgres
-- module: sdkwork-deploy
-- purpose: Add the application template marketplace catalog tables to a schema
--   that was baselined before they existed. Baseline additions only reach an
--   empty schema, so a database initialized earlier keeps the older shape
--   forever; already-initialized schemas converge through this ordered
--   migration, which adds exactly the three surviving catalog tables in
--   dependency order (category -> template -> version) with their marketplace
--   indexes and comments. Acquisition, payment and entitlement are owned by the
--   sdkwork-order order center and sdkwork-payment, so this migration creates
--   the module's catalog tables only.
-- reversible: true
-- rollback: down-migration
-- transactional: true
-- lock: lightweight
-- lock_timeout: 2s
-- statement_timeout: 30s

BEGIN;

CREATE TABLE IF NOT EXISTS deploy_app_template_category (
    id              BIGINT        NOT NULL,
    uuid            VARCHAR(36)   NOT NULL,
    tenant_id       BIGINT        NOT NULL,
    organization_id BIGINT        NOT NULL DEFAULT 0,
    -- 父分类；NULL 为根分类。只支持一层父子（浏览树由两层拼出），
    -- 不引入递归，删除父分类前必须先处理子分类。
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
    CONSTRAINT pk_deploy_app_template_category PRIMARY KEY (id),
    CONSTRAINT uk_deploy_app_template_category_uuid UNIQUE (uuid),
    CONSTRAINT fk_deploy_app_template_category_parent FOREIGN KEY (parent_id) REFERENCES deploy_app_template_category(id),
    CONSTRAINT chk_deploy_app_template_category_status CHECK (status IN ('ACTIVE', 'DISABLED'))
);

-- key 的唯一性必须带谓词：软删除的分类要释放 key 供同 key 重建，
-- 而 PostgreSQL 的 UNIQUE 约束不带谓词，只能用部分唯一索引表达。
CREATE UNIQUE INDEX IF NOT EXISTS uk_deploy_app_template_category_tenant_key
    ON deploy_app_template_category (tenant_id, category_key)
    WHERE deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_deploy_app_template_category_browse
    ON deploy_app_template_category (tenant_id, parent_id, status, sort_order, id)
    WHERE deleted_at IS NULL;

CREATE TABLE IF NOT EXISTS deploy_app_template (
    id               BIGINT        NOT NULL,
    uuid             VARCHAR(36)   NOT NULL,
    tenant_id        BIGINT        NOT NULL,
    organization_id  BIGINT        NOT NULL DEFAULT 0,
    category_id      BIGINT        NOT NULL,
    -- 作者（发布者）用户主体，与 created_by 语义分离：created_by 记录
    -- 下发命令的人，author_user_id 是列表归属，转移后可以不同。
    author_user_id   BIGINT        NOT NULL,
    -- 来源应用，引用而非外键：模板是发布出来的独立商品，不随 deploy_app
    -- 的行生命周期级联（同 deploy_dns_zone.provider_account_id 的理由）。
    app_uuid         VARCHAR(36)   NOT NULL,
    -- 模板类型：对话式项目创作产物的形态。APP 应用、PPT、视频都以项目
    -- 文件的方式进行对话创作生成，同一张模板表承载，类型只作分类维度，
    -- 不改变生命周期/权益语义。词汇表开放扩展（新增形态 = 基线 CHECK
    -- 追加一个值 + 契约枚举同步）。
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
    CONSTRAINT pk_deploy_app_template PRIMARY KEY (id),
    CONSTRAINT uk_deploy_app_template_uuid UNIQUE (uuid),
    CONSTRAINT fk_deploy_app_template_category FOREIGN KEY (category_id) REFERENCES deploy_app_template_category(id),
    CONSTRAINT chk_deploy_app_template_type CHECK (template_type IN ('APP', 'PPT', 'VIDEO')),
    CONSTRAINT chk_deploy_app_template_visibility CHECK (visibility IN ('PUBLIC', 'PRIVATE')),
    CONSTRAINT chk_deploy_app_template_pricing CHECK (pricing_model IN ('FREE', 'PAID')),
    -- 付费模板必须标价；免费模板价格恒为 0，防止"免费但标价"的脏数据。
    CONSTRAINT chk_deploy_app_template_price CHECK (
        (pricing_model = 'PAID' AND price_minor > 0) OR (pricing_model = 'FREE' AND price_minor = 0)
    ),
    CONSTRAINT chk_deploy_app_template_status CHECK (
        status IN ('DRAFT', 'PENDING_REVIEW', 'PUBLISHED', 'REJECTED', 'DISABLED')
    )
);

CREATE UNIQUE INDEX IF NOT EXISTS uk_deploy_app_template_tenant_key
    ON deploy_app_template (tenant_id, template_key)
    WHERE deleted_at IS NULL;

-- 市场浏览主路径：租户 + 状态 + 可见性 + 分类，排序固定 updated_at DESC, id。
CREATE INDEX IF NOT EXISTS idx_deploy_app_template_marketplace
    ON deploy_app_template (tenant_id, status, visibility, category_id, updated_at DESC, id)
    WHERE deleted_at IS NULL;

-- 按模板类型浏览的市场路径：类型是市场的一级 facet（APP/PPT/VIDEO 页签），
-- 与分类筛选平级，值得自己的索引前缀。
CREATE INDEX IF NOT EXISTS idx_deploy_app_template_marketplace_type
    ON deploy_app_template (tenant_id, status, visibility, template_type, updated_at DESC, id)
    WHERE deleted_at IS NULL;

-- 作者工作台列表路径。
CREATE INDEX IF NOT EXISTS idx_deploy_app_template_author
    ON deploy_app_template (tenant_id, author_user_id, status, updated_at DESC, id)
    WHERE deleted_at IS NULL;

CREATE TABLE IF NOT EXISTS deploy_app_template_version (
    id                  BIGINT        NOT NULL,
    uuid                VARCHAR(36)   NOT NULL,
    tenant_id           BIGINT        NOT NULL,
    organization_id     BIGINT        NOT NULL DEFAULT 0,
    template_id         BIGINT        NOT NULL,
    -- 作者可读的版本串。命名跟 deploy_build_template.template_version 对齐：
    -- 审计块还有一条乐观锁列也叫 version，业务列若同名会让基线建表直接
    -- 报 42701（column specified more than once）。
    template_version    VARCHAR(64)   NOT NULL,
    changelog           TEXT          NOT NULL DEFAULT '',
    -- 打包产物引用（deploy_artifact.uuid），未打包前为 NULL。
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
    CONSTRAINT pk_deploy_app_template_version PRIMARY KEY (id),
    CONSTRAINT uk_deploy_app_template_version_uuid UNIQUE (uuid),
    CONSTRAINT fk_deploy_app_template_version_template FOREIGN KEY (template_id) REFERENCES deploy_app_template(id),
    CONSTRAINT chk_deploy_app_template_version_status CHECK (status IN ('DRAFT', 'PUBLISHED', 'WITHDRAWN'))
);

CREATE UNIQUE INDEX IF NOT EXISTS uk_deploy_app_template_version_number
    ON deploy_app_template_version (template_id, template_version)
    WHERE deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_deploy_app_template_version_list
    ON deploy_app_template_version (tenant_id, template_id, status, created_at DESC, id)
    WHERE deleted_at IS NULL;

COMMENT ON TABLE deploy_app_template_category IS '应用模板市场分类；平台管理员维护，浏览树最多两层';
COMMENT ON COLUMN deploy_app_template_category.category_key IS '租户内稳定的分类标识，模板引用与统计都以它为准';
COMMENT ON TABLE deploy_app_template IS '应用模板：作者发布的可获取应用形态，含可见性、定价与审核状态';
COMMENT ON COLUMN deploy_app_template.app_uuid IS '来源 deploy_app.uuid，引用不级联：模板独立于应用行存在';
COMMENT ON COLUMN deploy_app_template.visibility IS 'PUBLIC 进市场浏览；PRIVATE 仅作者与自己租户可见';
COMMENT ON COLUMN deploy_app_template.pricing_model IS 'FREE 零元直接获取；PAID 模板经 sdkwork-order 下单、sdkwork-payment 支付，权益由订单中心持有';
COMMENT ON COLUMN deploy_app_template.price_minor IS '标价，货币最小单位（分）；FREE 恒为 0';
COMMENT ON COLUMN deploy_app_template.status IS 'DRAFT→PENDING_REVIEW→PUBLISHED；REJECTED 退回作者；DISABLED 平台下架';
COMMENT ON TABLE deploy_app_template_version IS '应用模板版本：一次发布的不可变快照，含产物引用与校验和';
COMMENT ON COLUMN deploy_app_template_version.artifact_uuid IS '打包产物 deploy_artifact.uuid；未打包为 NULL';

COMMIT;
