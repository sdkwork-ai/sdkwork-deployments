-- sdkwork:migration
-- id: 0019_deploy_certificate_archive
-- engine: postgres
-- module: deploy
-- purpose: Record the durable archive of an issued certificate version in Drive.
--
--   Issuing a certificate and storing its files in Drive are two different
--   failures: Drive can be unreachable while the CA is healthy, and vice versa.
--   Making the archive a row written in the **same transaction** as the
--   certificate version is what turns "archive after issuance" into a promise
--   the system can keep: a version that exists always has an archive intent, and
--   a failed upload is retried instead of disappearing with the request.
--
--   `bundle_kind` is `PUBLIC` (certificate files plus a manifest) or `FULL`
--   (additionally the private key, only under an explicit policy). One row per
--   (version, kind) keeps a retry idempotent.
--
-- reversible: false
-- rollback: forward-fix (dropping this table would lose the Drive node identity
--   of every archived version; the bytes remain in Drive but the control plane
--   could no longer name them)
-- transactional: true
-- lock: none (new table)
-- lock_timeout: 2s
-- statement_timeout: 30s

CREATE TABLE IF NOT EXISTS deploy_certificate_archive (
    id                      BIGINT       NOT NULL,
    uuid                    VARCHAR(36)  NOT NULL,
    tenant_id               BIGINT       NOT NULL,
    certificate_id          BIGINT       NOT NULL,
    certificate_version_id  BIGINT       NOT NULL,
    bundle_kind             VARCHAR(8)   NOT NULL DEFAULT 'PUBLIC',
    status                  VARCHAR(16)  NOT NULL DEFAULT 'PENDING',
    drive_space_id          VARCHAR(64),
    drive_folder_node_id    VARCHAR(64),
    drive_manifest_node_id  VARCHAR(64),
    attempt_count           INTEGER      NOT NULL DEFAULT 0,
    next_attempt_at         TIMESTAMPTZ,
    last_error_code         VARCHAR(64),
    manifest_sha256         VARCHAR(64),
    created_at              TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    updated_at              TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    version                 BIGINT       NOT NULL DEFAULT 1,
    PRIMARY KEY (id),
    CONSTRAINT uk_deploy_certificate_archive_uuid UNIQUE (uuid),
    CONSTRAINT uk_deploy_certificate_archive_version_kind UNIQUE (certificate_version_id, bundle_kind),
    CONSTRAINT fk_deploy_certificate_archive_certificate
        FOREIGN KEY (certificate_id) REFERENCES deploy_certificate(id),
    CONSTRAINT fk_deploy_certificate_archive_version
        FOREIGN KEY (certificate_version_id) REFERENCES deploy_certificate_version(id),
    CONSTRAINT chk_deploy_certificate_archive_kind CHECK (bundle_kind IN ('PUBLIC', 'FULL')),
    CONSTRAINT chk_deploy_certificate_archive_status
        CHECK (status IN ('PENDING', 'STORING', 'ARCHIVED', 'FAILED', 'SKIPPED')),
    -- An archived row must name the space and folder it lives in; anything else
    -- is a report an operator cannot act on.
    CONSTRAINT chk_deploy_certificate_archive_located CHECK (
        status <> 'ARCHIVED'
        OR (drive_space_id IS NOT NULL AND drive_folder_node_id IS NOT NULL)
    ),
    CONSTRAINT chk_deploy_certificate_archive_attempts CHECK (attempt_count BETWEEN 0 AND 100),
    CONSTRAINT chk_deploy_certificate_archive_manifest CHECK (
        manifest_sha256 IS NULL OR manifest_sha256 ~ '^[0-9a-f]{64}$'
    ),
    CONSTRAINT chk_deploy_certificate_archive_version CHECK (version >= 1)
);

CREATE INDEX IF NOT EXISTS idx_deploy_certificate_archive_due
    ON deploy_certificate_archive (status, next_attempt_at, id)
    WHERE status IN ('PENDING', 'STORING');

CREATE INDEX IF NOT EXISTS idx_deploy_certificate_archive_certificate
    ON deploy_certificate_archive (tenant_id, certificate_id, created_at DESC);
