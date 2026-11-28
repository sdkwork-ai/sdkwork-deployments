-- sdkwork:migration
-- id: 0024_deploy_app_capability
-- engine: postgres
-- module: deploy
-- purpose: Add the application capability domain: the six configurable
--   dimensions an application exposes when it is opened as an AI workspace
--   (work modes with bound tools, scene capsules with bound ecosystem assets,
--   the model pool selection, the in-app market shelf, MCP/iframe embeds), the
--   immutable configuration revisions that reviews and releases point at, the
--   review ledger, and the localized text store. Before this migration an
--   application could be built and delivered but had nowhere to declare what a
--   user sees when they open it. See
--   docs/architecture/tech/TECH-application-capability-model.md.
-- reversible: true
-- rollback: down-migration (the paired down migration drops the eleven tables
--   in reverse dependency order; no existing table is altered, so the down
--   migration cannot damage pre-existing rows)
-- transactional: true
-- lock: access-exclusive on the eleven newly created tables only; no existing
--   table is read or written, so no live traffic is blocked
-- lock_timeout: 2s
-- statement_timeout: 30s

BEGIN;

-- The 1:1 configuration root of one application.
--
-- Why it is a separate table instead of columns on `deploy_app`:
-- `deploy_app` is the *delivery* ledger — kind, status, revisions, domains —
-- written by the factory and read by the gateway. The capability document has
-- its own lifecycle (`draft → pending_review → published`), its own reviewer,
-- and its own immutable version line. Folding those columns into `deploy_app`
-- would put two independent state machines in one row and would make every
-- factory write carry an experience-config lock.
--
-- `organization_id` is 0 when the capability inherits the application's owner
-- level; it is only set when a tenant narrows the configuration to one
-- organization.
CREATE TABLE IF NOT EXISTS deploy_app_capability (
    id                    BIGINT       NOT NULL,
    uuid                  VARCHAR(36)  NOT NULL,
    tenant_id             BIGINT       NOT NULL,
    organization_id       BIGINT       NOT NULL DEFAULT 0,
    app_id                BIGINT       NOT NULL,
    -- The platform lifecycle vocabulary (`APPLICATION_IDENTITY_SPEC.md` §5):
    -- draft → pending_review → approved → published, plus deprecated/retired.
    -- `approved` exists so a two-step flow (approve now, publish later) does not
    -- need a new state; this domain's decide command publishes in one step and
    -- passes through `approved` inside the same transaction.
    capability_status     VARCHAR(24)  NOT NULL DEFAULT 'draft',
    -- Who may discover and open the application: `private` (creator only),
    -- `organization`, `tenant`, `enterprise` (only enterprises that hold an
    -- active enterprise order — owned by sdkwork-iam), `public`.
    distribution_scope    VARCHAR(16)  NOT NULL DEFAULT 'private',
    -- The 256x256 branded icon of the workspace. External CDN/media URL; the
    -- asset itself is owned by the media module, so no FK.
    avatar_url            VARCHAR(2048),
    -- The IAM OAuth client that serves this application (iam_application_installation_credential.client_id).
    -- A string reference, not a foreign key: IAM owns that table's lifecycle, and the
    -- capability document must survive a client rotation (the reference is updated by the
    -- application's own provisioning flow, never cascaded from another module).
    oauth_client_id       VARCHAR(128),
    -- Points at `deploy_app_work_mode.mode_key` of the same application. It is
    -- a *key*, not a row id, because a mode may be renamed while its key is
    -- stable, and because a dangling pointer must be a service-level validation
    -- error rather than a cascade that deletes the configuration root.
    default_work_mode_key VARCHAR(64),
    -- Newest revision of the document (telemetry pointer).
    current_revision_id   BIGINT,
    -- The revision that is live. Kept when a later draft is being edited so the
    -- published configuration never disappears from under a running workspace.
    published_revision_id BIGINT,
    published_at          TIMESTAMPTZ,
    -- Read-time upgrade marker for `..._revision.snapshot`.
    config_schema_version INTEGER      NOT NULL DEFAULT 1,
    metadata              JSONB        NOT NULL DEFAULT '{}',
    created_by            BIGINT,
    updated_by            BIGINT,
    created_at            TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    updated_at            TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    deleted_at            TIMESTAMPTZ,
    version               BIGINT       NOT NULL DEFAULT 1,
    CONSTRAINT pk_deploy_app_capability PRIMARY KEY (id),
    CONSTRAINT uk_deploy_app_capability_uuid UNIQUE (uuid),
    CONSTRAINT fk_deploy_app_capability_app
        FOREIGN KEY (app_id) REFERENCES deploy_app(id),
    CONSTRAINT chk_deploy_app_capability_status CHECK (capability_status IN (
        'draft', 'pending_review', 'approved', 'published', 'deprecated', 'retired'
    )),
    CONSTRAINT chk_deploy_app_capability_scope CHECK (distribution_scope IN (
        'private', 'organization', 'tenant', 'enterprise', 'public'
    )),
    -- Published means published: both pointers are present. The converse is not
    -- asserted, because a published application keeps its pointer while the next
    -- draft is edited (status already moved back to `draft`).
    CONSTRAINT chk_deploy_app_capability_published CHECK (
        capability_status <> 'published'
        OR (published_revision_id IS NOT NULL AND published_at IS NOT NULL)
    ),
    CONSTRAINT chk_deploy_app_capability_work_mode_key CHECK (
        default_work_mode_key IS NULL
        OR default_work_mode_key ~ '^[a-z][a-z0-9_-]{0,63}$'
    ),
    CONSTRAINT chk_deploy_app_capability_schema_version CHECK (config_schema_version >= 1),
    CONSTRAINT chk_deploy_app_capability_version CHECK (version >= 1)
);

-- One capability per application; a soft-deleted root releases the slot.
CREATE UNIQUE INDEX IF NOT EXISTS uk_deploy_app_capability_app
    ON deploy_app_capability (app_id)
    WHERE deleted_at IS NULL;

-- Owner inventory: "my applications' capability states".
CREATE INDEX IF NOT EXISTS idx_deploy_app_capability_owner
    ON deploy_app_capability (tenant_id, organization_id, capability_status, updated_at DESC, id)
    WHERE deleted_at IS NULL;

-- Publishing feed.
CREATE INDEX IF NOT EXISTS idx_deploy_app_capability_published
    ON deploy_app_capability (tenant_id, published_at DESC, id)
    WHERE capability_status = 'published' AND deleted_at IS NULL;

-- Immutable configuration revision. Append-only: the repository never issues an
-- UPDATE or DELETE against this table; the down migration is the only removal
-- path.
--
-- `snapshot` is the complete six-dimension document as one JSONB object. It is
-- a *snapshot*, not a second authority: the live tables remain authoritative for
-- "what is configured right now", and a snapshot exists so that a review, a
-- release, a rollback and an export all name exactly the bytes that were
-- approved. `snapshot_sha256` is the identity used to prove that.
CREATE TABLE IF NOT EXISTS deploy_app_capability_revision (
    id            BIGINT      NOT NULL,
    uuid          VARCHAR(36) NOT NULL,
    tenant_id     BIGINT      NOT NULL,
    organization_id BIGINT    NOT NULL DEFAULT 0,
    capability_id BIGINT      NOT NULL,
    app_id        BIGINT      NOT NULL,
    revision_no   INTEGER     NOT NULL,
    -- Which step produced this revision: a draft save, a review submission, or a
    -- publish. Recorded so an operator can tell "the bytes I approved" from "the
    -- bytes the editor saved afterwards".
    revision_source VARCHAR(16) NOT NULL DEFAULT 'DRAFT',
    snapshot      JSONB       NOT NULL,
    snapshot_sha256 VARCHAR(64) NOT NULL,
    snapshot_schema_version INTEGER NOT NULL DEFAULT 1,
    created_by    BIGINT,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT pk_deploy_app_capability_revision PRIMARY KEY (id),
    CONSTRAINT uk_deploy_app_capability_revision_uuid UNIQUE (uuid),
    CONSTRAINT fk_deploy_app_capability_revision_capability
        FOREIGN KEY (capability_id) REFERENCES deploy_app_capability(id),
    CONSTRAINT fk_deploy_app_capability_revision_app
        FOREIGN KEY (app_id) REFERENCES deploy_app(id),
    CONSTRAINT chk_deploy_app_capability_revision_no CHECK (revision_no > 0),
    CONSTRAINT chk_deploy_app_capability_revision_source CHECK (revision_source IN (
        'DRAFT', 'REVIEW', 'PUBLISH'
    )),
    CONSTRAINT chk_deploy_app_capability_revision_digest CHECK (
        snapshot_sha256 ~ '^[0-9a-f]{64}$'
    ),
    CONSTRAINT chk_deploy_app_capability_revision_document CHECK (
        jsonb_typeof(snapshot) = 'object'
    ),
    CONSTRAINT chk_deploy_app_capability_revision_schema_version CHECK (
        snapshot_schema_version >= 1
    ),
    CONSTRAINT chk_deploy_app_capability_revision_organization CHECK (organization_id >= 0)
);

CREATE UNIQUE INDEX IF NOT EXISTS uk_deploy_app_capability_revision_no
    ON deploy_app_capability_revision (capability_id, revision_no);

-- The same bytes are never stored twice for one application; a re-save of an
-- unchanged document reuses the existing revision instead of inflating history.
CREATE UNIQUE INDEX IF NOT EXISTS uk_deploy_app_capability_revision_digest
    ON deploy_app_capability_revision (capability_id, snapshot_sha256);

CREATE INDEX IF NOT EXISTS idx_deploy_app_capability_revision_list
    ON deploy_app_capability_revision (tenant_id, app_id, revision_no DESC, id);

-- Review ledger. One row per submission attempt; the decision lives on the row
-- it decided, so the history of rejections is not lost when a later attempt is
-- approved.
CREATE TABLE IF NOT EXISTS deploy_app_capability_review (
    id             BIGINT       NOT NULL,
    uuid           VARCHAR(36)  NOT NULL,
    tenant_id      BIGINT       NOT NULL,
    organization_id BIGINT      NOT NULL DEFAULT 0,
    capability_id  BIGINT       NOT NULL,
    app_id         BIGINT       NOT NULL,
    -- The exact revision under review. A later draft edit cannot change what the
    -- reviewer is looking at.
    revision_id    BIGINT       NOT NULL,
    attempt_no     INTEGER      NOT NULL,
    review_status  VARCHAR(16)  NOT NULL DEFAULT 'pending',
    submitted_by   BIGINT       NOT NULL,
    submitted_at   TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    reviewer_id    BIGINT,
    decided_at     TIMESTAMPTZ,
    -- Rejection reason family (`content_policy`, `security`, `design`,
    -- `functional`, `other`), so a rejection is actionable instead of free text.
    decision_code  VARCHAR(32),
    decision_note  VARCHAR(2000),
    metadata       JSONB        NOT NULL DEFAULT '{}',
    created_by     BIGINT,
    updated_by     BIGINT,
    created_at     TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    updated_at     TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    deleted_at     TIMESTAMPTZ,
    version        BIGINT       NOT NULL DEFAULT 1,
    CONSTRAINT pk_deploy_app_capability_review PRIMARY KEY (id),
    CONSTRAINT uk_deploy_app_capability_review_uuid UNIQUE (uuid),
    CONSTRAINT fk_deploy_app_capability_review_capability
        FOREIGN KEY (capability_id) REFERENCES deploy_app_capability(id),
    CONSTRAINT fk_deploy_app_capability_review_app
        FOREIGN KEY (app_id) REFERENCES deploy_app(id),
    CONSTRAINT fk_deploy_app_capability_review_revision
        FOREIGN KEY (revision_id) REFERENCES deploy_app_capability_revision(id),
    CONSTRAINT chk_deploy_app_capability_review_status CHECK (review_status IN (
        'pending', 'approved', 'rejected', 'withdrawn'
    )),
    CONSTRAINT chk_deploy_app_capability_review_attempt CHECK (attempt_no > 0),
    -- A decided review must name who decided and when; a pending one must not.
    CONSTRAINT chk_deploy_app_capability_review_decision CHECK (
        (review_status = 'pending' AND reviewer_id IS NULL AND decided_at IS NULL)
        OR (review_status IN ('approved', 'rejected')
            AND reviewer_id IS NOT NULL AND decided_at IS NOT NULL)
        OR review_status = 'withdrawn'
    ),
    CONSTRAINT chk_deploy_app_capability_review_code CHECK (
        decision_code IS NULL
        OR decision_code IN ('content_policy', 'security', 'design', 'functional', 'other')
    ),
    CONSTRAINT chk_deploy_app_capability_review_version CHECK (version >= 1)
);

-- At most one open submission per application: a second submit while one is
-- pending is a 409, not a queue of duplicates.
CREATE UNIQUE INDEX IF NOT EXISTS uk_deploy_app_capability_review_pending
    ON deploy_app_capability_review (capability_id)
    WHERE review_status = 'pending' AND deleted_at IS NULL;

CREATE UNIQUE INDEX IF NOT EXISTS uk_deploy_app_capability_review_attempt
    ON deploy_app_capability_review (capability_id, attempt_no)
    WHERE deleted_at IS NULL;

-- Review queue (oldest first) and per-application history (newest first).
CREATE INDEX IF NOT EXISTS idx_deploy_app_capability_review_queue
    ON deploy_app_capability_review (tenant_id, review_status, submitted_at, id)
    WHERE deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_deploy_app_capability_review_app
    ON deploy_app_capability_review (tenant_id, app_id, submitted_at DESC, id)
    WHERE deleted_at IS NULL;

-- Localized document text.
--
-- Why one table instead of one per resource: the six-dimension document is
-- replaced as a whole by `PUT .../capability`, so the text rows of a mode or a
-- scene are rewritten in the same transaction that rewrites the parent. A single
-- store keyed by the business key (`mode_key` / `scene_key`) therefore matches
-- the write model and keeps the document's translatable surface in one place;
-- `scope_kind` + `field_name` CHECKs keep the shape honest.
--
-- The FK is to the configuration root, not to the child row: the root is the
-- unit of replacement, and a child key may disappear and reappear during an
-- edit without the text rows needing to be dropped and re-created.
CREATE TABLE IF NOT EXISTS deploy_app_capability_text (
    id            BIGINT       NOT NULL,
    uuid          VARCHAR(36)  NOT NULL,
    tenant_id     BIGINT       NOT NULL,
    organization_id BIGINT     NOT NULL DEFAULT 0,
    capability_id BIGINT       NOT NULL,
    app_id        BIGINT       NOT NULL,
    scope_kind    VARCHAR(16)  NOT NULL,
    -- Business key of the document node: '' for the root, `mode_key` for a work
    -- mode, `scene_key` for a scene.
    scope_key     VARCHAR(64)  NOT NULL DEFAULT '',
    field_name    VARCHAR(32)  NOT NULL,
    -- BCP 47, normalized on write (`zh-CN`, `en-US`).
    locale        VARCHAR(16)  NOT NULL,
    text_value    TEXT         NOT NULL,
    metadata      JSONB        NOT NULL DEFAULT '{}',
    created_by    BIGINT,
    updated_by    BIGINT,
    created_at    TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    updated_at    TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    deleted_at    TIMESTAMPTZ,
    version       BIGINT       NOT NULL DEFAULT 1,
    CONSTRAINT pk_deploy_app_capability_text PRIMARY KEY (id),
    CONSTRAINT uk_deploy_app_capability_text_uuid UNIQUE (uuid),
    CONSTRAINT fk_deploy_app_capability_text_capability
        FOREIGN KEY (capability_id) REFERENCES deploy_app_capability(id),
    CONSTRAINT fk_deploy_app_capability_text_app
        FOREIGN KEY (app_id) REFERENCES deploy_app(id),
    CONSTRAINT chk_deploy_app_capability_text_scope CHECK (scope_kind IN (
        'CAPABILITY', 'WORK_MODE', 'SCENE'
    )),
    -- The field vocabulary is per scope: a work mode has a name and a
    -- description, the root has the slogan and the composer placeholder, a scene
    -- has a title and a subtitle. One CHECK keeps three vocabularies from drifting
    -- into one another.
    CONSTRAINT chk_deploy_app_capability_text_field CHECK (
        (scope_kind = 'CAPABILITY' AND field_name IN ('slogan', 'input_placeholder', 'summary'))
        OR (scope_kind = 'WORK_MODE' AND field_name IN ('name', 'description'))
        OR (scope_kind = 'SCENE' AND field_name IN ('title', 'subtitle'))
    ),
    CONSTRAINT chk_deploy_app_capability_text_scope_key CHECK (
        (scope_kind = 'CAPABILITY' AND scope_key = '')
        OR (scope_kind <> 'CAPABILITY' AND scope_key <> '')
    ),
    CONSTRAINT chk_deploy_app_capability_text_locale CHECK (
        locale ~ '^[a-z]{2,3}(-[A-Za-z0-9]{2,8})*$'
    ),
    CONSTRAINT chk_deploy_app_capability_text_length CHECK (char_length(text_value) <= 4000),
    CONSTRAINT chk_deploy_app_capability_text_version CHECK (version >= 1)
);

CREATE UNIQUE INDEX IF NOT EXISTS uk_deploy_app_capability_text
    ON deploy_app_capability_text (capability_id, scope_kind, scope_key, field_name, locale)
    WHERE deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_deploy_app_capability_text_scope
    ON deploy_app_capability_text (tenant_id, app_id, scope_kind, scope_key, locale)
    WHERE deleted_at IS NULL;

-- Work mode: the industry System Prompt plus the tools pre-bound to it.
CREATE TABLE IF NOT EXISTS deploy_app_work_mode (
    id             BIGINT       NOT NULL,
    uuid           VARCHAR(36)  NOT NULL,
    tenant_id      BIGINT       NOT NULL,
    organization_id BIGINT      NOT NULL DEFAULT 0,
    capability_id  BIGINT       NOT NULL,
    app_id         BIGINT       NOT NULL,
    -- Stable business key. The display name lives in `..._capability_text`, so a
    -- rename never changes the key a scene or a default pointer uses.
    mode_key       VARCHAR(64)  NOT NULL,
    system_prompt  TEXT         NOT NULL,
    icon_ref       VARCHAR(256),
    sort_order     INTEGER      NOT NULL DEFAULT 0,
    is_default     BOOLEAN      NOT NULL DEFAULT FALSE,
    mode_status    VARCHAR(16)  NOT NULL DEFAULT 'active',
    metadata       JSONB        NOT NULL DEFAULT '{}',
    created_by     BIGINT,
    updated_by     BIGINT,
    created_at     TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    updated_at     TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    deleted_at     TIMESTAMPTZ,
    version        BIGINT       NOT NULL DEFAULT 1,
    CONSTRAINT pk_deploy_app_work_mode PRIMARY KEY (id),
    CONSTRAINT uk_deploy_app_work_mode_uuid UNIQUE (uuid),
    CONSTRAINT fk_deploy_app_work_mode_capability
        FOREIGN KEY (capability_id) REFERENCES deploy_app_capability(id),
    CONSTRAINT fk_deploy_app_work_mode_app
        FOREIGN KEY (app_id) REFERENCES deploy_app(id),
    CONSTRAINT chk_deploy_app_work_mode_key CHECK (mode_key ~ '^[a-z][a-z0-9_-]{0,63}$'),
    CONSTRAINT chk_deploy_app_work_mode_prompt CHECK (
        char_length(system_prompt) BETWEEN 1 AND 32000
    ),
    CONSTRAINT chk_deploy_app_work_mode_sort CHECK (sort_order >= 0),
    CONSTRAINT chk_deploy_app_work_mode_status CHECK (mode_status IN ('active', 'disabled')),
    CONSTRAINT chk_deploy_app_work_mode_version CHECK (version >= 1)
);

CREATE UNIQUE INDEX IF NOT EXISTS uk_deploy_app_work_mode_key
    ON deploy_app_work_mode (capability_id, mode_key)
    WHERE deleted_at IS NULL;

-- One default mode per application. `mode_status = 'active'` is part of the
-- predicate: disabling the default must not keep the slot occupied.
CREATE UNIQUE INDEX IF NOT EXISTS uk_deploy_app_work_mode_default
    ON deploy_app_work_mode (capability_id)
    WHERE is_default AND mode_status = 'active' AND deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_deploy_app_work_mode_list
    ON deploy_app_work_mode (tenant_id, app_id, sort_order, id)
    WHERE deleted_at IS NULL;

-- Tools pre-bound to a work mode.
--
-- (tool_kind, source_module) is a closed pairing: a built-in tool belongs to the
-- host runtime (`deploy`), an MCP tool to `sdkwork-mcp`. `tool_ref` is the
-- owning module's stable business key (`server_key` / `connector_key` for MCP),
-- never a row id, because the target module may rebuild the row.
CREATE TABLE IF NOT EXISTS deploy_app_work_mode_tool (
    id             BIGINT       NOT NULL,
    uuid           VARCHAR(36)  NOT NULL,
    tenant_id      BIGINT       NOT NULL,
    organization_id BIGINT      NOT NULL DEFAULT 0,
    capability_id  BIGINT       NOT NULL,
    app_id         BIGINT       NOT NULL,
    work_mode_id   BIGINT       NOT NULL,
    tool_kind      VARCHAR(16)  NOT NULL,
    tool_ref       VARCHAR(191) NOT NULL,
    source_module  VARCHAR(16)  NOT NULL,
    policy         JSONB        NOT NULL DEFAULT '{}',
    sort_order     INTEGER      NOT NULL DEFAULT 0,
    metadata       JSONB        NOT NULL DEFAULT '{}',
    created_by     BIGINT,
    updated_by     BIGINT,
    created_at     TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    updated_at     TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    deleted_at     TIMESTAMPTZ,
    version        BIGINT       NOT NULL DEFAULT 1,
    CONSTRAINT pk_deploy_app_work_mode_tool PRIMARY KEY (id),
    CONSTRAINT uk_deploy_app_work_mode_tool_uuid UNIQUE (uuid),
    CONSTRAINT fk_deploy_app_work_mode_tool_capability
        FOREIGN KEY (capability_id) REFERENCES deploy_app_capability(id),
    CONSTRAINT fk_deploy_app_work_mode_tool_app
        FOREIGN KEY (app_id) REFERENCES deploy_app(id),
    CONSTRAINT fk_deploy_app_work_mode_tool_mode
        FOREIGN KEY (work_mode_id) REFERENCES deploy_app_work_mode(id),
    CONSTRAINT chk_deploy_app_work_mode_tool_kind CHECK (tool_kind IN ('BUILTIN', 'MCP')),
    CONSTRAINT chk_deploy_app_work_mode_tool_module CHECK (
        (tool_kind = 'BUILTIN' AND source_module = 'deploy')
        OR (tool_kind = 'MCP' AND source_module = 'mcp')
    ),
    CONSTRAINT chk_deploy_app_work_mode_tool_ref CHECK (
        tool_ref ~ '^[A-Za-z0-9][A-Za-z0-9._:/-]{0,190}$'
    ),
    CONSTRAINT chk_deploy_app_work_mode_tool_sort CHECK (sort_order >= 0),
    CONSTRAINT chk_deploy_app_work_mode_tool_version CHECK (version >= 1)
);

CREATE UNIQUE INDEX IF NOT EXISTS uk_deploy_app_work_mode_tool
    ON deploy_app_work_mode_tool (work_mode_id, tool_kind, tool_ref)
    WHERE deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_deploy_app_work_mode_tool_mode
    ON deploy_app_work_mode_tool (tenant_id, app_id, work_mode_id, sort_order, id)
    WHERE deleted_at IS NULL;

-- Scene capsule: the "things you may want to do" chips above the composer.
CREATE TABLE IF NOT EXISTS deploy_app_scene (
    id              BIGINT       NOT NULL,
    uuid            VARCHAR(36)  NOT NULL,
    tenant_id       BIGINT       NOT NULL,
    organization_id BIGINT       NOT NULL DEFAULT 0,
    capability_id   BIGINT       NOT NULL,
    app_id          BIGINT       NOT NULL,
    scene_key       VARCHAR(64)  NOT NULL,
    -- The prompt template pre-filled into the composer when the chip is clicked.
    prompt_template TEXT         NOT NULL,
    icon_ref        VARCHAR(256),
    sort_order      INTEGER      NOT NULL DEFAULT 0,
    -- Featured slot on the market page (WorkBuddy's 精选场景).
    is_featured     BOOLEAN      NOT NULL DEFAULT FALSE,
    scene_status    VARCHAR(16)  NOT NULL DEFAULT 'active',
    metadata        JSONB        NOT NULL DEFAULT '{}',
    created_by      BIGINT,
    updated_by      BIGINT,
    created_at      TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    deleted_at      TIMESTAMPTZ,
    version         BIGINT       NOT NULL DEFAULT 1,
    CONSTRAINT pk_deploy_app_scene PRIMARY KEY (id),
    CONSTRAINT uk_deploy_app_scene_uuid UNIQUE (uuid),
    CONSTRAINT fk_deploy_app_scene_capability
        FOREIGN KEY (capability_id) REFERENCES deploy_app_capability(id),
    CONSTRAINT fk_deploy_app_scene_app
        FOREIGN KEY (app_id) REFERENCES deploy_app(id),
    CONSTRAINT chk_deploy_app_scene_key CHECK (scene_key ~ '^[a-z][a-z0-9_-]{0,63}$'),
    CONSTRAINT chk_deploy_app_scene_prompt CHECK (
        char_length(prompt_template) BETWEEN 1 AND 8000
    ),
    CONSTRAINT chk_deploy_app_scene_sort CHECK (sort_order >= 0),
    CONSTRAINT chk_deploy_app_scene_status CHECK (scene_status IN ('active', 'disabled')),
    CONSTRAINT chk_deploy_app_scene_version CHECK (version >= 1)
);

CREATE UNIQUE INDEX IF NOT EXISTS uk_deploy_app_scene_key
    ON deploy_app_scene (capability_id, scene_key)
    WHERE deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_deploy_app_scene_list
    ON deploy_app_scene (tenant_id, app_id, sort_order, id)
    WHERE deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_deploy_app_scene_featured
    ON deploy_app_scene (tenant_id, app_id, sort_order, id)
    WHERE is_featured AND deleted_at IS NULL;

-- Ecosystem assets a scene starts in one click.
--
-- The reference is the owning module's stable business key. `asset_snapshot`
-- exists only so the console can render a chip without calling five modules; it
-- is never authoritative — a read that needs the asset's truth resolves it in
-- the owning module.
CREATE TABLE IF NOT EXISTS deploy_app_scene_asset (
    id             BIGINT       NOT NULL,
    uuid           VARCHAR(36)  NOT NULL,
    tenant_id      BIGINT       NOT NULL,
    organization_id BIGINT      NOT NULL DEFAULT 0,
    capability_id  BIGINT       NOT NULL,
    app_id         BIGINT       NOT NULL,
    scene_id       BIGINT       NOT NULL,
    asset_kind     VARCHAR(24)  NOT NULL,
    source_module  VARCHAR(16)  NOT NULL,
    asset_ref      VARCHAR(191) NOT NULL,
    asset_snapshot JSONB        NOT NULL DEFAULT '{}',
    sort_order     INTEGER      NOT NULL DEFAULT 0,
    metadata       JSONB        NOT NULL DEFAULT '{}',
    created_by     BIGINT,
    updated_by     BIGINT,
    created_at     TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    updated_at     TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    deleted_at     TIMESTAMPTZ,
    version        BIGINT       NOT NULL DEFAULT 1,
    CONSTRAINT pk_deploy_app_scene_asset PRIMARY KEY (id),
    CONSTRAINT uk_deploy_app_scene_asset_uuid UNIQUE (uuid),
    CONSTRAINT fk_deploy_app_scene_asset_capability
        FOREIGN KEY (capability_id) REFERENCES deploy_app_capability(id),
    CONSTRAINT fk_deploy_app_scene_asset_app
        FOREIGN KEY (app_id) REFERENCES deploy_app(id),
    CONSTRAINT fk_deploy_app_scene_asset_scene
        FOREIGN KEY (scene_id) REFERENCES deploy_app_scene(id),
    CONSTRAINT chk_deploy_app_scene_asset_kind CHECK (asset_kind IN (
        'EXPERT', 'EXPERT_TEAM', 'SKILL', 'CONNECTOR', 'CASE'
    )),
    -- Closed pairing between the asset kind and the module that owns it. Without
    -- it, an `EXPERT` could point at an MCP server key and the failure would only
    -- surface at runtime.
    CONSTRAINT chk_deploy_app_scene_asset_module CHECK (
        (asset_kind IN ('EXPERT', 'EXPERT_TEAM') AND source_module = 'agents')
        OR (asset_kind = 'SKILL' AND source_module = 'skills')
        OR (asset_kind = 'CONNECTOR' AND source_module = 'mcp')
        OR (asset_kind = 'CASE' AND source_module = 'appstore')
    ),
    CONSTRAINT chk_deploy_app_scene_asset_ref CHECK (
        asset_ref ~ '^[A-Za-z0-9][A-Za-z0-9._:/@-]{0,190}$'
    ),
    CONSTRAINT chk_deploy_app_scene_asset_sort CHECK (sort_order >= 0),
    CONSTRAINT chk_deploy_app_scene_asset_version CHECK (version >= 1)
);

CREATE UNIQUE INDEX IF NOT EXISTS uk_deploy_app_scene_asset
    ON deploy_app_scene_asset (scene_id, asset_kind, asset_ref)
    WHERE deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_deploy_app_scene_asset_scene
    ON deploy_app_scene_asset (tenant_id, app_id, scene_id, sort_order, id)
    WHERE deleted_at IS NULL;

-- Asset inventory: "which applications bind this skill" is a real operator
-- question and needs its own index.
CREATE INDEX IF NOT EXISTS idx_deploy_app_scene_asset_ref
    ON deploy_app_scene_asset (tenant_id, asset_kind, asset_ref)
    WHERE deleted_at IS NULL;

-- Model pool selection: which models this application offers, in which order,
-- with one default.
CREATE TABLE IF NOT EXISTS deploy_app_model_binding (
    id             BIGINT       NOT NULL,
    uuid           VARCHAR(36)  NOT NULL,
    tenant_id      BIGINT       NOT NULL,
    organization_id BIGINT      NOT NULL DEFAULT 0,
    capability_id  BIGINT       NOT NULL,
    app_id         BIGINT       NOT NULL,
    provider_code  VARCHAR(32)  NOT NULL,
    model_key      VARCHAR(128) NOT NULL,
    sort_order     INTEGER      NOT NULL DEFAULT 0,
    is_default     BOOLEAN      NOT NULL DEFAULT FALSE,
    binding_status VARCHAR(16)  NOT NULL DEFAULT 'active',
    metadata       JSONB        NOT NULL DEFAULT '{}',
    created_by     BIGINT,
    updated_by     BIGINT,
    created_at     TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    updated_at     TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    deleted_at     TIMESTAMPTZ,
    version        BIGINT       NOT NULL DEFAULT 1,
    CONSTRAINT pk_deploy_app_model_binding PRIMARY KEY (id),
    CONSTRAINT uk_deploy_app_model_binding_uuid UNIQUE (uuid),
    CONSTRAINT fk_deploy_app_model_binding_capability
        FOREIGN KEY (capability_id) REFERENCES deploy_app_capability(id),
    CONSTRAINT fk_deploy_app_model_binding_app
        FOREIGN KEY (app_id) REFERENCES deploy_app(id),
    CONSTRAINT chk_deploy_app_model_binding_provider CHECK (
        provider_code ~ '^[a-z][a-z0-9_-]{1,31}$'
    ),
    CONSTRAINT chk_deploy_app_model_binding_model CHECK (
        model_key ~ '^[A-Za-z0-9][A-Za-z0-9._:/-]{0,127}$'
    ),
    CONSTRAINT chk_deploy_app_model_binding_sort CHECK (sort_order >= 0),
    CONSTRAINT chk_deploy_app_model_binding_status CHECK (binding_status IN ('active', 'disabled')),
    CONSTRAINT chk_deploy_app_model_binding_version CHECK (version >= 1)
);

CREATE UNIQUE INDEX IF NOT EXISTS uk_deploy_app_model_binding
    ON deploy_app_model_binding (capability_id, provider_code, model_key)
    WHERE deleted_at IS NULL;

CREATE UNIQUE INDEX IF NOT EXISTS uk_deploy_app_model_binding_default
    ON deploy_app_model_binding (capability_id)
    WHERE is_default AND binding_status = 'active' AND deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_deploy_app_model_binding_list
    ON deploy_app_model_binding (tenant_id, app_id, sort_order, id)
    WHERE deleted_at IS NULL;

-- The in-app market shelf: what a user can discover inside the application.
--
-- Distinct from `deploy_app_scene_asset` by purpose, not by reference shape: a
-- scene binding starts a capability in one click, a market entry makes it
-- discoverable. The same expert may be both, and the two rows answer different
-- questions.
CREATE TABLE IF NOT EXISTS deploy_app_market_asset (
    id             BIGINT       NOT NULL,
    uuid           VARCHAR(36)  NOT NULL,
    tenant_id      BIGINT       NOT NULL,
    organization_id BIGINT      NOT NULL DEFAULT 0,
    capability_id  BIGINT       NOT NULL,
    app_id         BIGINT       NOT NULL,
    section        VARCHAR(24)  NOT NULL,
    asset_kind     VARCHAR(24)  NOT NULL,
    source_module  VARCHAR(16)  NOT NULL,
    asset_ref      VARCHAR(191) NOT NULL,
    category_code  VARCHAR(64),
    sort_order     INTEGER      NOT NULL DEFAULT 0,
    is_featured    BOOLEAN      NOT NULL DEFAULT FALSE,
    asset_status   VARCHAR(16)  NOT NULL DEFAULT 'active',
    asset_snapshot JSONB        NOT NULL DEFAULT '{}',
    metadata       JSONB        NOT NULL DEFAULT '{}',
    created_by     BIGINT,
    updated_by     BIGINT,
    created_at     TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    updated_at     TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    deleted_at     TIMESTAMPTZ,
    version        BIGINT       NOT NULL DEFAULT 1,
    CONSTRAINT pk_deploy_app_market_asset PRIMARY KEY (id),
    CONSTRAINT uk_deploy_app_market_asset_uuid UNIQUE (uuid),
    CONSTRAINT fk_deploy_app_market_asset_capability
        FOREIGN KEY (capability_id) REFERENCES deploy_app_capability(id),
    CONSTRAINT fk_deploy_app_market_asset_app
        FOREIGN KEY (app_id) REFERENCES deploy_app(id),
    CONSTRAINT chk_deploy_app_market_asset_section CHECK (section IN (
        'experts', 'skills', 'connectors', 'featured_scenes', 'cases'
    )),
    CONSTRAINT chk_deploy_app_market_asset_kind CHECK (asset_kind IN (
        'EXPERT', 'EXPERT_TEAM', 'SKILL', 'CONNECTOR', 'CASE'
    )),
    -- The shelf and the asset kind are one decision: an expert shelf cannot list
    -- a connector, and the featured-scene slot is a featured expert or team.
    CONSTRAINT chk_deploy_app_market_asset_shelf CHECK (
        (section = 'experts' AND asset_kind IN ('EXPERT', 'EXPERT_TEAM'))
        OR (section = 'skills' AND asset_kind = 'SKILL')
        OR (section = 'connectors' AND asset_kind = 'CONNECTOR')
        OR (section = 'featured_scenes' AND asset_kind IN ('EXPERT', 'EXPERT_TEAM'))
        OR (section = 'cases' AND asset_kind = 'CASE')
    ),
    CONSTRAINT chk_deploy_app_market_asset_module CHECK (
        (asset_kind IN ('EXPERT', 'EXPERT_TEAM') AND source_module = 'agents')
        OR (asset_kind = 'SKILL' AND source_module = 'skills')
        OR (asset_kind = 'CONNECTOR' AND source_module = 'mcp')
        OR (asset_kind = 'CASE' AND source_module = 'appstore')
    ),
    CONSTRAINT chk_deploy_app_market_asset_ref CHECK (
        asset_ref ~ '^[A-Za-z0-9][A-Za-z0-9._:/@-]{0,190}$'
    ),
    CONSTRAINT chk_deploy_app_market_asset_category CHECK (
        category_code IS NULL OR category_code ~ '^[a-z][a-z0-9_-]{0,63}$'
    ),
    CONSTRAINT chk_deploy_app_market_asset_sort CHECK (sort_order >= 0),
    CONSTRAINT chk_deploy_app_market_asset_status CHECK (asset_status IN ('active', 'disabled')),
    CONSTRAINT chk_deploy_app_market_asset_version CHECK (version >= 1)
);

CREATE UNIQUE INDEX IF NOT EXISTS uk_deploy_app_market_asset
    ON deploy_app_market_asset (capability_id, section, asset_kind, asset_ref)
    WHERE deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_deploy_app_market_asset_list
    ON deploy_app_market_asset (tenant_id, app_id, section, sort_order, id)
    WHERE deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_deploy_app_market_asset_ref
    ON deploy_app_market_asset (tenant_id, asset_kind, asset_ref)
    WHERE deleted_at IS NULL;

-- Embedded existing systems: an MCP app the workspace can call, or an iframe
-- surface it can show. Credentials stay in sdkwork-iam; this row records the
-- reference and the minimal scope set the embed may use.
CREATE TABLE IF NOT EXISTS deploy_app_embed (
    id             BIGINT       NOT NULL,
    uuid           VARCHAR(36)  NOT NULL,
    tenant_id      BIGINT       NOT NULL,
    organization_id BIGINT      NOT NULL DEFAULT 0,
    capability_id  BIGINT       NOT NULL,
    app_id         BIGINT       NOT NULL,
    embed_key      VARCHAR(64)  NOT NULL,
    embed_kind     VARCHAR(16)  NOT NULL,
    -- MCP server business key (`server_key`), owned by sdkwork-mcp.
    server_ref     VARCHAR(191),
    -- Absolute https URL for the iframe variant.
    endpoint_url   VARCHAR(2048),
    transport      VARCHAR(16),
    auth_mode      VARCHAR(16)  NOT NULL DEFAULT 'none',
    scopes_json    JSONB        NOT NULL DEFAULT '[]',
    embed_status   VARCHAR(16)  NOT NULL DEFAULT 'active',
    sort_order     INTEGER      NOT NULL DEFAULT 0,
    metadata       JSONB        NOT NULL DEFAULT '{}',
    created_by     BIGINT,
    updated_by     BIGINT,
    created_at     TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    updated_at     TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    deleted_at     TIMESTAMPTZ,
    version        BIGINT       NOT NULL DEFAULT 1,
    CONSTRAINT pk_deploy_app_embed PRIMARY KEY (id),
    CONSTRAINT uk_deploy_app_embed_uuid UNIQUE (uuid),
    CONSTRAINT fk_deploy_app_embed_capability
        FOREIGN KEY (capability_id) REFERENCES deploy_app_capability(id),
    CONSTRAINT fk_deploy_app_embed_app
        FOREIGN KEY (app_id) REFERENCES deploy_app(id),
    CONSTRAINT chk_deploy_app_embed_key CHECK (embed_key ~ '^[a-z][a-z0-9_-]{0,63}$'),
    CONSTRAINT chk_deploy_app_embed_kind CHECK (embed_kind IN ('MCP_APP', 'IFRAME')),
    -- Exactly one address per kind: an MCP app names a server, an iframe names a
    -- URL, and neither carries the other's field.
    CONSTRAINT chk_deploy_app_embed_address CHECK (
        (embed_kind = 'MCP_APP' AND server_ref IS NOT NULL AND endpoint_url IS NULL
            AND transport IS NOT NULL)
        OR (embed_kind = 'IFRAME' AND endpoint_url IS NOT NULL AND server_ref IS NULL
            AND endpoint_url ~ '^https://')
    ),
    CONSTRAINT chk_deploy_app_embed_transport CHECK (
        transport IS NULL OR transport IN ('stdio', 'http', 'sse')
    ),
    CONSTRAINT chk_deploy_app_embed_auth CHECK (auth_mode IN ('none', 'oauth', 'api_key')),
    CONSTRAINT chk_deploy_app_embed_scopes CHECK (jsonb_typeof(scopes_json) = 'array'),
    CONSTRAINT chk_deploy_app_embed_sort CHECK (sort_order >= 0),
    CONSTRAINT chk_deploy_app_embed_status CHECK (embed_status IN ('active', 'disabled')),
    CONSTRAINT chk_deploy_app_embed_version CHECK (version >= 1)
);

CREATE UNIQUE INDEX IF NOT EXISTS uk_deploy_app_embed_key
    ON deploy_app_embed (capability_id, embed_key)
    WHERE deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_deploy_app_embed_list
    ON deploy_app_embed (tenant_id, app_id, sort_order, id)
    WHERE deleted_at IS NULL;

COMMENT ON TABLE deploy_app_capability IS
    '应用能力配置根（每应用 1 行）。写属主 deploy-platform；organization_id>0 表示配置限定在组织级，0 表示继承应用所有者层级。';
COMMENT ON COLUMN deploy_app_capability.capability_status IS
    '平台生命周期词表：draft→pending_review→approved→published，另有 deprecated/retired。published 必须有 published_revision_id 与 published_at；对已发布配置的写入把状态退回 draft 但保留 published_revision_id。';
COMMENT ON COLUMN deploy_app_capability.distribution_scope IS
    'private 仅创建者 / organization 组织内 / tenant 租户内 / enterprise 仅持有效企业单的企业 / public 公开。企业单本身归 sdkwork-iam，本列只声明意图。';
COMMENT ON COLUMN deploy_app_capability.default_work_mode_key IS
    '指向同应用 deploy_app_work_mode.mode_key；刻意不建外键，删除模式由服务层在同一事务里清指针，避免级联删掉配置根。';
COMMENT ON TABLE deploy_app_capability_revision IS
    '不可变配置快照（append-only，禁止 UPDATE/DELETE）。snapshot 是六维配置的完整 JSONB 文档，供审核、发布、回滚、导出使用；snapshot_sha256 是「审的就是这些字节」的证明。';
COMMENT ON TABLE deploy_app_capability_review IS
    '审核台账：每次送审一行，裁决写在被裁决的那一行上，因此被驳回的历史不会因为后来通过而丢失。每应用同刻至多一条 pending。';
COMMENT ON TABLE deploy_app_capability_text IS
    '应用能力文档的本地化文案。按 (scope_kind, scope_key, field_name, locale) 唯一；scope_key 为空串表示配置根，否则是 mode_key/scene_key 业务键。';
COMMENT ON TABLE deploy_app_work_mode IS
    '工作模式：行业 System Prompt + 预绑定工具；mode_key 稳定，展示名在 deploy_app_capability_text。';
COMMENT ON TABLE deploy_app_work_mode_tool IS
    '工作模式预绑定工具。(tool_kind, source_module) 为闭合配对：BUILTIN→deploy、MCP→mcp；tool_ref 为对端稳定业务键。';
COMMENT ON TABLE deploy_app_scene IS
    '场景胶囊：输入框上方的快捷提示词模板；is_featured 是市场页精选位。';
COMMENT ON TABLE deploy_app_scene_asset IS
    '场景一键启动所绑定的生态资产。(asset_kind, source_module) 闭合配对，asset_ref 为对端业务键，asset_snapshot 仅供展示。';
COMMENT ON TABLE deploy_app_model_binding IS
    '应用可用模型清单与展示顺序；每应用至多一个默认模型。模型属性归平台模型目录，本表不复制。';
COMMENT ON TABLE deploy_app_market_asset IS
    '应用内市场货架：可发现的专家/专家团/技能/连接器/精选场景/案例。section 与 asset_kind 由 CHECK 钉死配对。';
COMMENT ON TABLE deploy_app_embed IS
    '内嵌已有功能：MCP_APP 引 server_key，IFRAME 引 https URL；凭据归 sdkwork-iam，本表只登记引用与最小 scope 集合。';

COMMIT;
