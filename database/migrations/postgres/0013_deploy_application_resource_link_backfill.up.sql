-- sdkwork:migration
-- id: 0013_deploy_application_resource_link_backfill
-- engine: postgres
-- module: sdkwork-deploy
-- purpose: Normalize the account bindings that only exist inside
--   `deploy_app.metadata.integrations` (schema
--   `sdkwork.application.integrations/v1`) into
--   `deploy_application_resource_link`, which becomes the authority.
--
--   The legacy document is a nested, unconstrained JSON object:
--
--     { "schema": "sdkwork.application.integrations/v1",
--       "linkedApplications": [...],
--       "environments": { "<environment>": {
--           "officialAccount": { "id", "appId", "displayName", "providerCode", … },
--           "miniProgram":     { "id", "appId", … },
--           "paymentAccounts": [ { "id", "accountNo", "providerCode",
--                                  "environment", "accountMode", "isDefault", … } ] } } }
--
--   The mapping, and nothing creative:
--
--     environment   <- the `environments` key, restricted to the five lifecycle
--                      values 0012's CHECK accepts
--     resource_kind <- official_account | mini_program | payment_account
--     resource_ref  <- official/mini: the provider-side AppID (`appId`)
--                      payment:        the merchant account number (`accountNo`)
--     provider_code <- `providerCode`, required for payment
--     is_default    <- `isDefault`, payment only; the first account of a provider
--                      per environment wins the default slot (0012's unique index
--                      allows exactly one)
--     metadata      <- the display snapshot plus the source account id and the
--                      `source` marker that makes this migration re-runnable and
--                      reversible
--
--   Idempotency: rows are addressed by `(app, environment, kind[, reference])`, the
--   identifier is a *deterministic* uuid derived from that address, and the insert
--   is guarded by `NOT EXISTS` plus `ON CONFLICT DO NOTHING`. Running this file
--   twice inserts nothing the second time; running it after a new binding was
--   written through the API leaves that binding alone.
--
--   Only parseable rows migrate. An entry whose `appId`/`accountNo`/`providerCode`
--   is missing, or whose environment key is not a lifecycle value, is left in the
--   document untouched — `tools/report_app_resource_links.sql` lists every one of
--   them with its reason, and the NOTICE below reports how many bindings landed.
-- reversible: true
-- rollback: down-migration (deletes exactly the rows this migration created,
--   identified by metadata.source; the source document is never modified)
-- transactional: true
-- lock: no schema lock; row locks only on the inserted bindings
-- lock_timeout: 2s
-- statement_timeout: 120s

BEGIN;

DO $backfill$
DECLARE
    inserted_count BIGINT;
BEGIN
    WITH documents AS (
        SELECT
            a.tenant_id,
            a.organization_id,
            a.id   AS app_internal_id,
            a.uuid AS app_uuid,
            lower(btrim(environment.key)) AS env_key,
            environment.value AS env_doc
        FROM deploy_app a
        -- `jsonb_each` needs an object; the CASE keeps a malformed document a
        -- no-op instead of a migration failure, because "cannot parse" is a
        -- reported outcome, not an error.
        CROSS JOIN LATERAL jsonb_each(
            CASE
                WHEN jsonb_typeof(a.metadata) = 'object'
                 AND jsonb_typeof(a.metadata -> 'integrations') = 'object'
                 AND jsonb_typeof(a.metadata -> 'integrations' -> 'environments') = 'object'
                THEN a.metadata -> 'integrations' -> 'environments'
                ELSE '{}'::jsonb
            END
        ) AS environment(key, value)
        WHERE a.deleted_at IS NULL
          -- Only the versioned document shape is migrated. A future schema value
          -- gets its own migration instead of being guessed at here.
          AND a.metadata -> 'integrations' ->> 'schema' = 'sdkwork.application.integrations/v1'
    ),
    channel_raw AS (
        SELECT d.tenant_id, d.organization_id, d.app_internal_id, d.app_uuid,
               d.env_key, d.env_doc, 'official_account'::text AS resource_kind,
               d.env_doc -> 'officialAccount' AS binding
        FROM documents d
        UNION ALL
        SELECT d.tenant_id, d.organization_id, d.app_internal_id, d.app_uuid,
               d.env_key, d.env_doc, 'mini_program'::text AS resource_kind,
               d.env_doc -> 'miniProgram' AS binding
        FROM documents d
    ),
    channel_scoped AS (
        SELECT
            tenant_id, organization_id, app_internal_id, app_uuid, env_key, resource_kind,
            btrim(binding ->> 'appId') AS resource_ref,
            nullif(btrim(coalesce(binding ->> 'providerCode', '')), '') AS provider_code,
            jsonb_strip_nulls(jsonb_build_object(
                'source', 'deploy_app.metadata.integrations',
                'sourceSchema', 'sdkwork.application.integrations/v1',
                'sourceAccountId', binding ->> 'id',
                'displayName', binding ->> 'displayName',
                'accountType', binding ->> 'accountType',
                'accountStatus', binding ->> 'status',
                'authorizationStatus', binding ->> 'authorizationStatus'
            )) AS metadata
        FROM channel_raw
        WHERE jsonb_typeof(env_doc) = 'object'
          AND jsonb_typeof(binding) = 'object'
    ),
    channel_candidates AS (
        SELECT *
        FROM channel_scoped
        WHERE env_key IN ('development', 'test', 'staging', 'demo', 'production')
          AND resource_ref <> ''
          AND char_length(resource_ref) <= 128
          AND (provider_code IS NULL OR char_length(provider_code) <= 32)
    ),
    payment_raw AS (
        SELECT d.tenant_id, d.organization_id, d.app_internal_id, d.app_uuid,
               d.env_key, d.env_doc, account.value AS binding
        FROM documents d
        CROSS JOIN LATERAL jsonb_array_elements(
            CASE
                WHEN jsonb_typeof(d.env_doc) = 'object'
                 AND jsonb_typeof(d.env_doc -> 'paymentAccounts') = 'array'
                THEN d.env_doc -> 'paymentAccounts'
                ELSE '[]'::jsonb
            END
        ) AS account(value)
    ),
    payment_scoped AS (
        SELECT
            tenant_id, organization_id, app_internal_id, app_uuid, env_key,
            'payment_account'::text AS resource_kind,
            btrim(binding ->> 'accountNo') AS resource_ref,
            nullif(btrim(coalesce(binding ->> 'providerCode', '')), '') AS provider_code,
            CASE
                WHEN jsonb_typeof(binding -> 'isDefault') = 'boolean'
                THEN (binding ->> 'isDefault')::boolean
                ELSE FALSE
            END AS is_default,
            jsonb_strip_nulls(jsonb_build_object(
                'source', 'deploy_app.metadata.integrations',
                'sourceSchema', 'sdkwork.application.integrations/v1',
                'sourceAccountId', binding ->> 'id',
                'accountName', binding ->> 'accountName',
                'accountMode', binding ->> 'accountMode',
                'paymentEnvironment', binding ->> 'environment',
                'settlementCurrency', binding ->> 'settlementCurrency',
                'countryCode', binding ->> 'countryCode'
            )) AS metadata
        FROM payment_raw
        WHERE jsonb_typeof(env_doc) = 'object'
          AND jsonb_typeof(binding) = 'object'
    ),
    payment_candidates AS (
        SELECT *
        FROM payment_scoped
        WHERE env_key IN ('development', 'test', 'staging', 'demo', 'production')
          AND resource_ref <> ''
          AND char_length(resource_ref) <= 128
          -- A payment account without a provider has no default slot and no
          -- provider to resolve against; it stays in the document and is reported.
          AND provider_code IS NOT NULL
          AND char_length(provider_code) <= 32
    ),
    candidates AS (
        SELECT tenant_id, organization_id, app_internal_id, app_uuid, env_key,
               resource_kind, resource_ref, provider_code, FALSE AS is_default, metadata
        FROM channel_candidates
        UNION ALL
        SELECT tenant_id, organization_id, app_internal_id, app_uuid, env_key,
               resource_kind, resource_ref, provider_code, is_default, metadata
        FROM payment_candidates
    ),
    ranked AS (
        SELECT
            c.*,
            row_number() OVER (
                PARTITION BY c.app_internal_id, c.env_key, c.resource_kind, c.resource_ref
                ORDER BY c.resource_ref
            ) AS reference_rank,
            -- At most one default per (app, environment, provider): the first
            -- account number wins the slot, the rest are still bound but not
            -- default. Dropping them instead would silently unbind a merchant
            -- account because a document listed two defaults.
            CASE
                WHEN c.resource_kind = 'payment_account' AND c.is_default
                THEN row_number() OVER (
                    PARTITION BY c.app_internal_id, c.env_key, c.provider_code
                    ORDER BY c.resource_ref
                )
            END AS default_rank
        FROM candidates c
    ),
    selected AS (
        SELECT
            r.tenant_id,
            r.organization_id,
            r.app_internal_id,
            r.env_key,
            r.resource_kind,
            r.resource_ref,
            r.provider_code,
            -- `default_rank` is NULL for a channel candidate (no default slot), and
            -- `NULL = 1` is NULL, which the NOT NULL column refuses. A channel
            -- binding is never the default.
            coalesce(r.default_rank = 1, FALSE) AS is_default,
            r.metadata
        FROM ranked r
        WHERE r.reference_rank = 1
          -- The natural key, exactly as the runtime addresses it: a
          -- single-instance kind is `(environment, kind)`, a payment account adds
          -- the reference. An existing row — created through the API after 0012,
          -- or by a previous run of this file — is never overwritten.
          AND NOT EXISTS (
              SELECT 1
              FROM deploy_application_resource_link existing
              WHERE existing.tenant_id = r.tenant_id
                AND existing.app_id = r.app_internal_id
                AND existing.env_key = r.env_key
                AND existing.resource_kind = r.resource_kind
                AND existing.deleted_at IS NULL
                AND (
                    r.resource_kind <> 'payment_account'
                    OR existing.resource_ref = r.resource_ref
                )
          )
    )
    INSERT INTO deploy_application_resource_link (
        id, tenant_id, organization_id, app_id, env_key, resource_kind,
        resource_ref, provider_code, is_default, binding_status, metadata,
        created_by, updated_by, created_at, updated_at, version
    )
    SELECT
        -- Deterministic uuid derived from the binding's address (DATABASE_SPEC
        -- §6.3 "documented deterministic remap"): the same document always maps
        -- to the same id, so a re-run addresses the same row and cannot duplicate
        -- it — including when the partial unique index no longer matches for a
        -- reason the address does not express.
        substr(h.digest, 1, 8) || '-' || substr(h.digest, 9, 4) || '-4'
            || substr(h.digest, 14, 3) || '-a' || substr(h.digest, 18, 3)
            || '-' || substr(h.digest, 21, 12) AS id,
        s.tenant_id,
        s.organization_id,
        s.app_internal_id,
        s.env_key,
        s.resource_kind,
        s.resource_ref,
        s.provider_code,
        s.is_default,
        'ACTIVE',
        s.metadata,
        NULL,
        NULL,
        now(),
        now(),
        1
    FROM selected s
    CROSS JOIN LATERAL (
        SELECT md5(
            s.tenant_id::text || '|' || s.app_internal_id::text || '|'
            || s.env_key || '|' || s.resource_kind || '|' || s.resource_ref
        ) AS digest
    ) h
    ON CONFLICT DO NOTHING;

    GET DIAGNOSTICS inserted_count = ROW_COUNT;
    RAISE NOTICE
        'deploy_application_resource_link backfill: % binding(s) normalized from deploy_app.metadata.integrations; '
        'unparseable entries remain in the source document and are listed by tools/report_app_resource_links.sql',
        inserted_count;
END
$backfill$;

COMMIT;
