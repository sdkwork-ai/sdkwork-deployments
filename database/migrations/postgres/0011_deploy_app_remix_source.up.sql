-- sdkwork:migration
-- id: 0011_deploy_app_remix_source
-- engine: postgres
-- module: sdkwork-deploy
-- purpose: An application can be remixed only when its source exists. Two facts
--   are missing from the schema:
--
--   - deploy_app.remix_enabled: the operator's switch. Default FALSE means an
--     existing application never silently becomes remixable.
--   - deploy_app_source: the ONE source a remix downloads. A source is either a
--     Git repository (with the repository-relative directory the code lives in)
--     or an uploaded archive artifact. It is a table of its own, not columns on
--     deploy_app and not a flag on deploy_source_repository, because:
--
--     * deploy_source_repository is a *collection* (unique on (app_id,
--       repo_key)): an app may register several repositories for builds, so
--       "which one is the source" cannot be expressed there without a partial
--       unique index that would forbid the second repository.
--     * an archive source has no home at all today: deploy_artifact is
--       tenant-scoped and shared by packages and releases.
--     * exactly-one-source is a database fact here (uk_deploy_app_source_app)
--       and the kind/pointer agreement is a CHECK, so a half-configured source
--       is rejected by PostgreSQL instead of by every caller.
--
--   READY vs MISSING is *derived* at read time from the referenced rows (repo
--   status and URL, artifact status) and is deliberately not stored: a stored
--   status would go stale the moment an artifact is retained or a repository is
--   soft-deleted, and nothing would ever refresh it.
-- reversible: true
-- rollback: down-migration (drops the table and the column added here)
-- transactional: true
-- lock: ACCESS EXCLUSIVE on deploy_app for the added column; the new table is
--   invisible to other sessions until COMMIT
-- lock_timeout: 2s
-- statement_timeout: 60s

BEGIN;

ALTER TABLE deploy_app
    ADD COLUMN IF NOT EXISTS remix_enabled BOOLEAN NOT NULL DEFAULT FALSE;

-- Supports "list the tenant's remixable applications" without scanning the whole
-- inventory. Partial on the flag, so it stays proportional to the enabled set.
CREATE INDEX IF NOT EXISTS idx_deploy_app_remix_enabled
    ON deploy_app (tenant_id)
    WHERE remix_enabled AND deleted_at IS NULL;

CREATE TABLE IF NOT EXISTS deploy_app_source (
    id                  BIGINT       NOT NULL,
    uuid                VARCHAR(36)  NOT NULL,
    tenant_id           BIGINT       NOT NULL,
    organization_id     BIGINT       NOT NULL DEFAULT 0,
    app_id              BIGINT       NOT NULL,
    -- GIT | ARCHIVE
    source_kind         VARCHAR(16)  NOT NULL,
    -- The Git repository this source reads from. Mutually exclusive with
    -- `archive_artifact_id`; which one is required is decided by `source_kind`.
    git_repository_id   BIGINT,
    -- The completed upload artifact this source reads from.
    archive_artifact_id BIGINT,
    -- Branch, tag or commit to download. NULL means the repository's
    -- `default_branch`, which is the value the console shows.
    source_ref          VARCHAR(255),
    -- Repository-relative directory the code lives in. NULL is the repository
    -- root. Stored as a normalized relative path (see the CHECK below) so the
    -- remix client never has to decide whether "apps/web/" and "apps/web" are
    -- the same directory.
    sub_path            VARCHAR(1000),
    created_by          BIGINT,
    updated_by          BIGINT,
    created_at          TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    updated_at          TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    deleted_at          TIMESTAMPTZ,
    version             BIGINT       NOT NULL DEFAULT 1,
    CONSTRAINT pk_deploy_app_source PRIMARY KEY (id),
    CONSTRAINT fk_deploy_app_source_app
        FOREIGN KEY (app_id) REFERENCES deploy_app(id),
    -- The referenced rows must outlive the reference. `deploy_source_repository`
    -- and `deploy_artifact` are soft-retired in place, so a hard delete here can
    -- only be an operator mistake; refusing it is the intended failure mode.
    CONSTRAINT fk_deploy_app_source_git_repository
        FOREIGN KEY (git_repository_id) REFERENCES deploy_source_repository(id),
    CONSTRAINT fk_deploy_app_source_archive_artifact
        FOREIGN KEY (archive_artifact_id) REFERENCES deploy_artifact(id),
    CONSTRAINT chk_deploy_app_source_kind
        CHECK (source_kind IN ('GIT', 'ARCHIVE')),
    -- Exactly one pointer, and the pointer the kind names. A row that carries
    -- both, or neither, is not a source: the remix download would have to guess
    -- which half to believe.
    CONSTRAINT chk_deploy_app_source_pointer CHECK (
        (source_kind = 'GIT' AND git_repository_id IS NOT NULL AND archive_artifact_id IS NULL)
        OR (source_kind = 'ARCHIVE' AND archive_artifact_id IS NOT NULL AND git_repository_id IS NULL)
    ),
    -- `source_ref` and `sub_path` describe a Git checkout; an archive already
    -- contains exactly one tree, extracted at the root.
    CONSTRAINT chk_deploy_app_source_git_scope CHECK (
        source_kind = 'GIT'
        OR (source_ref IS NULL AND sub_path IS NULL)
    ),
    CONSTRAINT chk_deploy_app_source_source_ref CHECK (
        source_ref IS NULL OR char_length(source_ref) BETWEEN 1 AND 255
    ),
    -- Relative, normalized, no traversal. Mirrors
    -- `sdkwork_deploy_core::normalize_app_source_sub_path`, which is what the
    -- service applies before the write; the constraint exists so a direct SQL
    -- write cannot store a path the remix client would resolve outside the
    -- checkout.
    CONSTRAINT chk_deploy_app_source_sub_path CHECK (
        sub_path IS NULL
        OR (
            char_length(sub_path) BETWEEN 1 AND 1000
            AND sub_path !~ '^/'
            AND sub_path !~ '//'
            AND sub_path !~ '(^|/)\.\.?(/|$)'
            AND sub_path ~ '^[A-Za-z0-9._-]+(/[A-Za-z0-9._-]+)*$'
        )
    )
);

CREATE UNIQUE INDEX IF NOT EXISTS uk_deploy_app_source_uuid
    ON deploy_app_source (uuid);

-- One source per application. Partial so a future "replace the source" that
-- retires the old row keeps working.
CREATE UNIQUE INDEX IF NOT EXISTS uk_deploy_app_source_app
    ON deploy_app_source (app_id)
    WHERE deleted_at IS NULL;

-- "Which applications use this repository / this artifact" — needed before a
-- repository is retired and to explain what an artifact is still backing.
CREATE INDEX IF NOT EXISTS idx_deploy_app_source_git_repository
    ON deploy_app_source (git_repository_id)
    WHERE git_repository_id IS NOT NULL AND deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_deploy_app_source_archive_artifact
    ON deploy_app_source (archive_artifact_id)
    WHERE archive_artifact_id IS NOT NULL AND deleted_at IS NULL;

COMMENT ON COLUMN deploy_app.remix_enabled IS
    '应用是否支持 remix；开启要求源码就绪（deploy_app_source 的 READY 判定）';

COMMENT ON TABLE deploy_app_source IS
    '应用 remix 源码（每个应用至多一行）：GIT 仓库（可指定仓库内子目录）或已上传压缩包制品';

COMMENT ON COLUMN deploy_app_source.sub_path IS
    '仓库内源码目录（相对路径，NULL 为仓库根）；规范化存储，禁止绝对路径与 .. 穿越';

COMMENT ON COLUMN deploy_app_source.source_ref IS
    '下载用的分支/标签/提交；NULL 表示使用仓库默认分支';

COMMENT ON COLUMN deploy_app_source.source_kind IS
    'GIT：git_repository_id 必填；ARCHIVE：archive_artifact_id 必填';

COMMIT;
