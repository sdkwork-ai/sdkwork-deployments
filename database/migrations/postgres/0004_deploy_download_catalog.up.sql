-- sdkwork:migration
-- id: 0004_deploy_download_catalog
-- engine: postgres
-- module: deploy
-- description: Adds the operator-authored download catalog and its per-installer
--   update configuration to the Deploy database authority. A catalog names one
--   product (product_key/name/version) and groups cards (one card per product
--   surface) and items (one item per platform/architecture installer). Each
--   item owns at most one update configuration, which is the piece that used to
--   live as a free-form JSON document inside the Cloud Router site settings: the
--   update provider, feed URL, release channel, minimum supported version,
--   check interval and staged rollout belong to one installer, not to the site.
--   Installers reuse the existing release domain (deploy_package,
--   deploy_artifact, deploy_release_channel) instead of duplicating artifacts or
--   channels, so a published package and the download page can never disagree.
-- reversible: true
-- rollback: down-migration drops the download catalog tables in reverse dependency order
-- transactional: true
-- lock: lightweight
-- lock_timeout: 2s
-- statement_timeout: 60s

BEGIN;

-- One download catalog per (tenant, organization, catalog_key). The site that
-- renders the catalog keeps only this key; the catalog itself is owned here.
CREATE TABLE IF NOT EXISTS deploy_download_catalog (
    id                  BIGINT       NOT NULL,
    uuid                VARCHAR(36)  NOT NULL,
    tenant_id           BIGINT       NOT NULL,
    organization_id     BIGINT       NOT NULL DEFAULT 0,
    catalog_key         VARCHAR(64)  NOT NULL,
    app_id              BIGINT       NULL,
    product_key         VARCHAR(64)  NOT NULL,
    product_name        VARCHAR(120) NOT NULL,
    product_version     VARCHAR(64)  NOT NULL,
    default_channel_key VARCHAR(32)  NOT NULL DEFAULT 'nightly',
    status              VARCHAR(16)  NOT NULL DEFAULT 'DRAFT',
    revision            BIGINT       NOT NULL DEFAULT 1,
    published_at        TIMESTAMPTZ  NULL,
    created_by          BIGINT       NULL,
    updated_by          BIGINT       NULL,
    created_at          TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    updated_at          TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    deleted_at          TIMESTAMPTZ  NULL,
    version             BIGINT       NOT NULL DEFAULT 1,
    CONSTRAINT pk_deploy_download_catalog PRIMARY KEY (id),
    CONSTRAINT uk_deploy_download_catalog_uuid UNIQUE (uuid),
    CONSTRAINT fk_deploy_download_catalog_app FOREIGN KEY (app_id) REFERENCES deploy_app(id),
    CONSTRAINT chk_deploy_download_catalog_status CHECK (status IN ('DRAFT', 'PUBLISHED', 'ARCHIVED')),
    CONSTRAINT chk_deploy_download_catalog_revision CHECK (revision >= 1)
);

CREATE UNIQUE INDEX IF NOT EXISTS uk_deploy_download_catalog_scope
    ON deploy_download_catalog (tenant_id, organization_id, catalog_key)
    WHERE deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_deploy_download_catalog_public
    ON deploy_download_catalog (tenant_id, catalog_key, status)
    WHERE deleted_at IS NULL;

COMMENT ON TABLE deploy_download_catalog IS 'Operator-authored download catalog; one row per tenant/organization/catalog_key';
COMMENT ON COLUMN deploy_download_catalog.catalog_key IS 'Stable key the rendering site stores instead of the catalog document';
COMMENT ON COLUMN deploy_download_catalog.product_key IS 'Client product identity compared by the desktop updater';
COMMENT ON COLUMN deploy_download_catalog.status IS 'DRAFT is invisible to the public surface; PUBLISHED is readable';
COMMENT ON COLUMN deploy_download_catalog.revision IS 'Increments on every published catalog change; feeds the public ETag';

-- One visual card inside a catalog (for example "Desktop" or "Mobile").
CREATE TABLE IF NOT EXISTS deploy_download_card (
    id                  BIGINT       NOT NULL,
    uuid                VARCHAR(36)  NOT NULL,
    tenant_id           BIGINT       NOT NULL,
    organization_id     BIGINT       NOT NULL DEFAULT 0,
    catalog_id          BIGINT       NOT NULL,
    card_key            VARCHAR(64)  NOT NULL,
    title               VARCHAR(255) NOT NULL,
    description         TEXT         NOT NULL DEFAULT '',
    icon                VARCHAR(32)  NOT NULL DEFAULT '',
    tone                VARCHAR(32)  NOT NULL DEFAULT '',
    kind                VARCHAR(32)  NOT NULL DEFAULT 'desktop',
    badge               VARCHAR(64)  NOT NULL DEFAULT '',
    primary_strategy    VARCHAR(32)  NOT NULL DEFAULT 'detected-platform',
    hidden              BOOLEAN      NOT NULL DEFAULT FALSE,
    sort_order          INTEGER      NOT NULL DEFAULT 0,
    created_by          BIGINT       NULL,
    updated_by          BIGINT       NULL,
    created_at          TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    updated_at          TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    deleted_at          TIMESTAMPTZ  NULL,
    version             BIGINT       NOT NULL DEFAULT 1,
    CONSTRAINT pk_deploy_download_card PRIMARY KEY (id),
    CONSTRAINT uk_deploy_download_card_uuid UNIQUE (uuid),
    CONSTRAINT fk_deploy_download_card_catalog FOREIGN KEY (catalog_id) REFERENCES deploy_download_catalog(id),
    CONSTRAINT chk_deploy_download_card_primary_strategy CHECK (primary_strategy IN ('detected-platform', 'first-available'))
);

CREATE UNIQUE INDEX IF NOT EXISTS uk_deploy_download_card_scope
    ON deploy_download_card (catalog_id, card_key)
    WHERE deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_deploy_download_card_order
    ON deploy_download_card (catalog_id, sort_order, id)
    WHERE deleted_at IS NULL;

COMMENT ON TABLE deploy_download_card IS 'Download page card grouping installers of one catalog';
COMMENT ON COLUMN deploy_download_card.hidden IS 'TRUE keeps the card out of the public catalog without deleting it';

-- One installer inside a card. package_id/artifact_id reuse the release domain;
-- download_source carries a direct href or a Drive reference for external files.
CREATE TABLE IF NOT EXISTS deploy_download_item (
    id                  BIGINT       NOT NULL,
    uuid                VARCHAR(36)  NOT NULL,
    tenant_id           BIGINT       NOT NULL,
    organization_id     BIGINT       NOT NULL DEFAULT 0,
    card_id             BIGINT       NOT NULL,
    item_key            VARCHAR(64)  NOT NULL,
    platform            VARCHAR(16)  NOT NULL,
    architecture        VARCHAR(16)  NOT NULL,
    label               VARCHAR(255) NOT NULL,
    file_name           VARCHAR(500) NOT NULL DEFAULT '',
    installer_version   VARCHAR(64)  NOT NULL DEFAULT '',
    release_tag         VARCHAR(64)  NOT NULL DEFAULT '',
    package_id          BIGINT       NULL,
    artifact_id         BIGINT       NULL,
    download_source     JSONB        NOT NULL DEFAULT '{}',
    size_bytes          BIGINT       NULL,
    checksum_sha256     VARCHAR(128) NULL,
    primary_action      BOOLEAN      NOT NULL DEFAULT FALSE,
    disabled            BOOLEAN      NOT NULL DEFAULT FALSE,
    unavailable_label   VARCHAR(255) NOT NULL DEFAULT '',
    sort_order          INTEGER      NOT NULL DEFAULT 0,
    created_by          BIGINT       NULL,
    updated_by          BIGINT       NULL,
    created_at          TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    updated_at          TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    deleted_at          TIMESTAMPTZ  NULL,
    version             BIGINT       NOT NULL DEFAULT 1,
    CONSTRAINT pk_deploy_download_item PRIMARY KEY (id),
    CONSTRAINT uk_deploy_download_item_uuid UNIQUE (uuid),
    CONSTRAINT fk_deploy_download_item_card FOREIGN KEY (card_id) REFERENCES deploy_download_card(id),
    CONSTRAINT fk_deploy_download_item_package FOREIGN KEY (package_id) REFERENCES deploy_package(id),
    CONSTRAINT fk_deploy_download_item_artifact FOREIGN KEY (artifact_id) REFERENCES deploy_artifact(id),
    CONSTRAINT chk_deploy_download_item_platform CHECK (platform IN ('windows', 'macos', 'linux', 'android', 'ios', 'harmonyos', 'docker', 'helm', 'generic')),
    CONSTRAINT chk_deploy_download_item_architecture CHECK (architecture IN ('x64', 'arm64', 'universal'))
);

CREATE UNIQUE INDEX IF NOT EXISTS uk_deploy_download_item_scope
    ON deploy_download_item (card_id, item_key)
    WHERE deleted_at IS NULL;

CREATE UNIQUE INDEX IF NOT EXISTS uk_deploy_download_item_target
    ON deploy_download_item (card_id, platform, architecture)
    WHERE deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_deploy_download_item_order
    ON deploy_download_item (card_id, sort_order, id)
    WHERE deleted_at IS NULL;

COMMENT ON TABLE deploy_download_item IS 'One downloadable installer; at most one update configuration per item';
COMMENT ON COLUMN deploy_download_item.download_source IS '{"href": "https://..."} or {"drive": {"spaceId","nodeId","shareToken"}}';
COMMENT ON COLUMN deploy_download_item.package_id IS 'Release-domain package used for feed generation and artifact redirect';

-- Per-installer update configuration. This is the row that makes two installers
-- of the same catalog follow different feeds, channels and rollouts.
CREATE TABLE IF NOT EXISTS deploy_download_update (
    id                          BIGINT       NOT NULL,
    uuid                        VARCHAR(36)  NOT NULL,
    tenant_id                   BIGINT       NOT NULL,
    organization_id             BIGINT       NOT NULL DEFAULT 0,
    item_id                     BIGINT       NOT NULL,
    enabled                     BOOLEAN      NOT NULL DEFAULT TRUE,
    provider                    VARCHAR(16)  NOT NULL DEFAULT 'deployments',
    feed_url                    VARCHAR(500) NOT NULL DEFAULT '',
    github_owner                VARCHAR(120) NOT NULL DEFAULT '',
    github_repo                 VARCHAR(120) NOT NULL DEFAULT '',
    channel_id                  BIGINT       NULL,
    channel_key                 VARCHAR(32)  NOT NULL DEFAULT 'nightly',
    channel_file                VARCHAR(64)  NOT NULL DEFAULT '',
    allowed_channels            JSONB        NOT NULL DEFAULT '[]',
    allow_prerelease            BOOLEAN      NOT NULL DEFAULT TRUE,
    allow_downgrade             BOOLEAN      NOT NULL DEFAULT FALSE,
    minimum_supported_version   VARCHAR(64)  NOT NULL DEFAULT '',
    check_interval_minutes      INTEGER      NOT NULL DEFAULT 10,
    rollout_percentage          INTEGER      NOT NULL DEFAULT 100,
    release_notes_url           VARCHAR(500) NOT NULL DEFAULT '',
    created_by                  BIGINT       NULL,
    updated_by                  BIGINT       NULL,
    created_at                  TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    updated_at                  TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    version                     BIGINT       NOT NULL DEFAULT 1,
    CONSTRAINT pk_deploy_download_update PRIMARY KEY (id),
    CONSTRAINT uk_deploy_download_update_uuid UNIQUE (uuid),
    CONSTRAINT uk_deploy_download_update_item UNIQUE (item_id),
    CONSTRAINT fk_deploy_download_update_item FOREIGN KEY (item_id) REFERENCES deploy_download_item(id),
    CONSTRAINT fk_deploy_download_update_channel FOREIGN KEY (channel_id) REFERENCES deploy_release_channel(id),
    CONSTRAINT chk_deploy_download_update_provider CHECK (provider IN ('deployments', 'generic', 'github', 'none')),
    CONSTRAINT chk_deploy_download_update_rollout CHECK (rollout_percentage BETWEEN 0 AND 100),
    CONSTRAINT chk_deploy_download_update_interval CHECK (check_interval_minutes BETWEEN 1 AND 1440)
);

COMMENT ON TABLE deploy_download_update IS 'Per-installer auto-update configuration; replaces the Cloud Router site-settings update block';
COMMENT ON COLUMN deploy_download_update.provider IS 'deployments = feed generated here; generic/github = external feed; none = updates paused';
COMMENT ON COLUMN deploy_download_update.channel_id IS 'Release domain channel whose current release backs a deployments feed';
COMMENT ON COLUMN deploy_download_update.rollout_percentage IS 'Staged rollout percentage applied to automatic update checks';

COMMIT;
