-- Migration: 0006_deploy_dns_zone_verification_file_reconciliation
-- Description: reconcile deploy databases provisioned before the
--   initialization-state consolidation: deploy_dns_zone_verification_file was
--   folded into the baseline (its loose migration never shipped in any
--   published binary), so existing databases lack it and the consolidated
--   binary's drift check fails with a missing_table error. The statement is
--   idempotent (IF NOT EXISTS) and mirrors the baseline DDL verbatim; fresh
--   installs already created the table and no-op here.
-- Author: SDKWork Deployments

CREATE TABLE IF NOT EXISTS deploy_dns_zone_verification_file (
    id              BIGINT        NOT NULL,
    uuid            VARCHAR(36)   NOT NULL,
    tenant_id       BIGINT        NOT NULL,
    organization_id BIGINT        NOT NULL DEFAULT 0,
    zone_id         BIGINT        NOT NULL,
    platform        VARCHAR(32)   NOT NULL,
    file_name       VARCHAR(128)  NOT NULL,
    content         VARCHAR(4096) NOT NULL,
    user_id         BIGINT,
    created_by      BIGINT,
    created_at      TIMESTAMPTZ   NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ   NOT NULL DEFAULT NOW(),
    version         BIGINT        NOT NULL DEFAULT 1,
    PRIMARY KEY (id),
    CONSTRAINT uk_deploy_dns_zone_verification_file_uuid UNIQUE (uuid),
    CONSTRAINT uk_deploy_dns_zone_verification_file_name UNIQUE (zone_id, file_name),
    CONSTRAINT fk_deploy_dns_zone_verification_file_zone FOREIGN KEY (zone_id) REFERENCES deploy_dns_zone(id),
    CONSTRAINT chk_deploy_dns_zone_verification_file_platform CHECK (platform IN (
        'WECHAT_OFFICIAL_ACCOUNT', 'WECHAT_MINI_PROGRAM', 'ALIPAY',
        'BYTEDANCE', 'APPLE', 'GOOGLE', 'CUSTOM'
    )),
    CONSTRAINT chk_deploy_dns_zone_verification_file_name CHECK (
        file_name ~ '^[A-Za-z0-9][A-Za-z0-9._-]*\.(txt|html|xml)$'
    ),
    CONSTRAINT chk_deploy_dns_zone_verification_file_content CHECK (content <> '')
);

COMMENT ON TABLE deploy_dns_zone_verification_file IS '域名归属校验文件；一个 DNS Zone 可登记多份（不同平台 / 同一平台换发的多份），按 hostname + 文件名原样公开发布';
