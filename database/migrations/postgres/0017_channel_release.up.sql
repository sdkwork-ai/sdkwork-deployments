-- sdkwork:migration
-- id: 0017_channel_release
-- engine: postgres
-- module: deploy
-- purpose: Make a provider release a first-class Deploy fact.
--
--   `deploy_release` describes what Deploy built and published to its own
--   delivery surface. It has no place for the WeChat facts: the uploaded commit,
--   the submitted audit, the provider version, the gray percentage, or which
--   release a rollback restored. Without them the publish pipeline can only
--   report "released" while the provider state is unknown, and a rollback cannot
--   name its target.
--
--   1. `deploy_application_resource_link` gains the credential mode and the
--      channel account pointer. Existing rows default to `self_managed`, which is
--      the only mode that existed before this migration.
--   2. `deploy_channel_release` records one provider release attempt per
--      environment and resource link. The unique `(resource_link_id, commit_id)`
--      index makes an upload retry idempotent instead of creating a second row.
--   3. `deploy_channel_template` maps a Deploy template version to the provider
--      template id that was added to the draft box.
--   4. `deploy_channel_domain` keeps the domain inventory and its provider sync
--      state, because WeChat's domain edits are quota-limited and must be
--      auditable rather than replayed blindly.
--
-- reversible: false
-- rollback: forward-fix (dropping the channel tables would lose the provider
--   commit/audit identity that support needs to reconcile a review)
-- transactional: true
-- lock: ACCESS EXCLUSIVE on deploy_application_resource_link for the added
--   columns; new tables are created empty
-- lock_timeout: 2s
-- statement_timeout: 60s
-- backfill: none. Every new column is nullable or defaulted and the new tables
--   start empty.

BEGIN;

ALTER TABLE deploy_application_resource_link
    ADD COLUMN IF NOT EXISTS access_mode VARCHAR(32) NOT NULL DEFAULT 'self_managed',
    ADD COLUMN IF NOT EXISTS channel_account_id VARCHAR(36),
    ADD COLUMN IF NOT EXISTS operator_platform_id VARCHAR(36),
    ADD COLUMN IF NOT EXISTS authorization_ref VARCHAR(36),
    ADD COLUMN IF NOT EXISTS capability_snapshot_json JSONB NOT NULL DEFAULT '{}';

ALTER TABLE deploy_application_resource_link
    DROP CONSTRAINT IF EXISTS chk_deploy_application_resource_link_access_mode;

ALTER TABLE deploy_application_resource_link
    ADD CONSTRAINT chk_deploy_application_resource_link_access_mode
    CHECK (access_mode IN ('self_managed', 'operator_authorized', 'external_provider'));

CREATE TABLE IF NOT EXISTS deploy_channel_release (
    id                      VARCHAR(36)  NOT NULL,
    tenant_id               BIGINT       NOT NULL,
    organization_id         BIGINT       NOT NULL DEFAULT 0,
    app_id                  BIGINT       NOT NULL,
    env_key                 VARCHAR(32)  NOT NULL,
    resource_link_id        VARCHAR(36)  NOT NULL,
    channel                 VARCHAR(32)  NOT NULL,
    access_mode             VARCHAR(32)  NOT NULL,
    provider_version        VARCHAR(64),
    template_id             VARCHAR(64),
    commit_id               VARCHAR(64),
    audit_id                VARCHAR(64),
    release_status          VARCHAR(24)  NOT NULL,
    gray_percentage         SMALLINT     NOT NULL DEFAULT 100,
    rollback_of_release_id  VARCHAR(36),
    error_code              VARCHAR(64),
    provider_payload_json   JSONB        NOT NULL DEFAULT '{}',
    submitted_at            TIMESTAMPTZ,
    released_at             TIMESTAMPTZ,
    created_by              BIGINT,
    updated_by              BIGINT,
    created_at              TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    updated_at              TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    version                 BIGINT       NOT NULL DEFAULT 1,
    CONSTRAINT pk_deploy_channel_release PRIMARY KEY (id),
    CONSTRAINT fk_deploy_channel_release_app
        FOREIGN KEY (app_id) REFERENCES deploy_app(id),
    CONSTRAINT chk_deploy_channel_release_channel
        CHECK (channel IN ('wechat_mini_program', 'wechat_official_account')),
    CONSTRAINT chk_deploy_channel_release_access_mode
        CHECK (access_mode IN ('self_managed', 'operator_authorized', 'external_provider')),
    CONSTRAINT chk_deploy_channel_release_status
        CHECK (release_status IN (
            'DRAFT', 'PROJECTED', 'UPLOADED', 'AUDIT_SUBMITTED', 'IN_REVIEW',
            'APPROVED', 'RELEASING', 'RELEASED', 'REJECTED', 'WITHDRAWN',
            'DEFERRED', 'FAILED', 'ROLLED_BACK', 'SUPERSEDED'
        )),
    CONSTRAINT chk_deploy_channel_release_gray
        CHECK (gray_percentage BETWEEN 0 AND 100)
);

CREATE INDEX IF NOT EXISTS idx_deploy_channel_release_app
    ON deploy_channel_release (tenant_id, app_id, env_key, release_status);

CREATE UNIQUE INDEX IF NOT EXISTS uk_deploy_channel_release_commit
    ON deploy_channel_release (resource_link_id, commit_id)
    WHERE commit_id IS NOT NULL;

CREATE TABLE IF NOT EXISTS deploy_channel_template (
    id                      VARCHAR(36)  NOT NULL,
    tenant_id               BIGINT       NOT NULL,
    organization_id         BIGINT       NOT NULL DEFAULT 0,
    app_id                  BIGINT       NOT NULL,
    operator_platform_id    VARCHAR(36),
    template_id             VARCHAR(64)  NOT NULL,
    template_version        VARCHAR(64),
    ext_json                JSONB        NOT NULL DEFAULT '{}',
    status                  VARCHAR(24)  NOT NULL DEFAULT 'ACTIVE',
    created_by              BIGINT,
    updated_by              BIGINT,
    created_at              TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    updated_at              TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    version                 BIGINT       NOT NULL DEFAULT 1,
    CONSTRAINT pk_deploy_channel_template PRIMARY KEY (id),
    CONSTRAINT fk_deploy_channel_template_app
        FOREIGN KEY (app_id) REFERENCES deploy_app(id)
);

CREATE UNIQUE INDEX IF NOT EXISTS uk_deploy_channel_template_provider
    ON deploy_channel_template (tenant_id, app_id, template_id);

CREATE TABLE IF NOT EXISTS deploy_channel_domain (
    id                      VARCHAR(36)  NOT NULL,
    tenant_id               BIGINT       NOT NULL,
    organization_id         BIGINT       NOT NULL DEFAULT 0,
    app_id                  BIGINT       NOT NULL,
    domain_kind             VARCHAR(24)  NOT NULL,
    domain                  TEXT         NOT NULL,
    bound                   BOOLEAN      NOT NULL DEFAULT FALSE,
    verified                BOOLEAN      NOT NULL DEFAULT FALSE,
    last_synced_at          TIMESTAMPTZ,
    created_by              BIGINT,
    updated_by              BIGINT,
    created_at              TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    updated_at              TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    version                 BIGINT       NOT NULL DEFAULT 1,
    CONSTRAINT pk_deploy_channel_domain PRIMARY KEY (id),
    CONSTRAINT fk_deploy_channel_domain_app
        FOREIGN KEY (app_id) REFERENCES deploy_app(id),
    CONSTRAINT chk_deploy_channel_domain_kind
        CHECK (domain_kind IN ('request', 'socket', 'upload', 'download', 'udp', 'webview'))
);

CREATE UNIQUE INDEX IF NOT EXISTS uk_deploy_channel_domain_value
    ON deploy_channel_domain (tenant_id, app_id, domain_kind, domain);

COMMIT;
