-- sdkwork:migration
-- id: 0002_deploy_domain_dns_record
-- engine: postgres
-- module: deploy
-- description: Stores the last synced snapshot of one Zone's DNS resolution
--   records. Until now "how does this hostname resolve" was answerable only by
--   opening the provider console: the control plane held no inventory of the
--   A / AAAA / CNAME records a Zone's hostnames resolve to, and the hostname
--   page could not show a resolution type or a resolution IP. The snapshot is
--   read from the provider through the Zone's cloud account
--   (`deploy_dns_zone.provider_account_id`, or the account center's choice for
--   the Zone's declared provider) and replaced whole per sync run, so the
--   table always answers with one provider answer rather than a merge of
--   several. `domain_id` is the hostname a record resolves, matched at sync
--   time with the wildcard semantics already applied; NULL is a record whose
--   owner matches no registered hostname, which the Zone-scoped read still
--   shows.
-- reversible: true
-- rollback: down-migration drops the snapshot table and its indexes
-- transactional: true
-- lock: access-exclusive
-- lock_timeout: 5s
-- statement_timeout: 120s

-- The table itself. Column shapes follow the module baseline: snowflake `id`
-- with a `uuid` mirror, tenant-scoped, soft-deletable, `version`-stamped.
CREATE TABLE IF NOT EXISTS deploy_domain_dns_record (
    id              BIGINT        NOT NULL,
    uuid            VARCHAR(36)   NOT NULL,
    tenant_id       BIGINT        NOT NULL,
    organization_id BIGINT        NOT NULL DEFAULT 0,
    zone_id         BIGINT        NOT NULL,
    domain_id       BIGINT,
    record_name     VARCHAR(253)  NOT NULL,
    record_type     VARCHAR(16)   NOT NULL,
    record_value    VARCHAR(1024) NOT NULL,
    ttl_seconds     INTEGER,
    priority        INTEGER,
    record_line     VARCHAR(64),
    dns_provider    VARCHAR(32)   NOT NULL,
    provider_account_id VARCHAR(128) NOT NULL,
    provider_record_ref VARCHAR(128),
    synced_at       TIMESTAMPTZ   NOT NULL DEFAULT NOW(),
    created_by      BIGINT,
    created_at      TIMESTAMPTZ   NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ   NOT NULL DEFAULT NOW(),
    version         BIGINT        NOT NULL DEFAULT 1,
    deleted_at      TIMESTAMPTZ,
    PRIMARY KEY (id),
    CONSTRAINT uk_deploy_domain_dns_record_uuid UNIQUE (uuid),
    CONSTRAINT fk_deploy_domain_dns_record_zone FOREIGN KEY (zone_id) REFERENCES deploy_dns_zone(id),
    CONSTRAINT fk_deploy_domain_dns_record_domain FOREIGN KEY (domain_id) REFERENCES deploy_domain(id),
    CONSTRAINT chk_deploy_domain_dns_record_owner CHECK (record_name <> ''),
    CONSTRAINT chk_deploy_domain_dns_record_provider_account CHECK (
        provider_account_id ~ '^[A-Za-z0-9][A-Za-z0-9_.:-]{1,127}$'
    )
);

COMMENT ON TABLE deploy_domain_dns_record IS 'DNS Zone 解析记录同步快照';
COMMENT ON COLUMN deploy_domain_dns_record.domain_id IS '该记录解析到的 hostname 行；owner 未匹配任何已登记 hostname 时为 NULL';
COMMENT ON COLUMN deploy_domain_dns_record.synced_at IS '本次快照从服务商读取的时间';

-- The Zone-scoped paged read (the resolution page's primary query), newest
-- snapshot first. `tenant_id` leads because every read is tenant-scoped.
CREATE INDEX IF NOT EXISTS idx_deploy_domain_dns_record_zone
    ON deploy_domain_dns_record (tenant_id, zone_id, synced_at DESC, id DESC)
    WHERE deleted_at IS NULL;

-- The hostname-scoped read (the detail page's query).
CREATE INDEX IF NOT EXISTS idx_deploy_domain_dns_record_domain
    ON deploy_domain_dns_record (tenant_id, domain_id)
    WHERE deleted_at IS NULL;
