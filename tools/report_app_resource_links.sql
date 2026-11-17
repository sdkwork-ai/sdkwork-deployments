-- Account-binding report for SDKWork Deploy.
--
-- Read-only. Run it after `db:migrate` (0013 normalizes the legacy document) and
-- whenever the integrations document has changed since:
--
--   psql "$SDKWORK_DATABASE_URL" -f tools/report_app_resource_links.sql
--
-- Answers the two questions the migration cannot answer on its own:
--
--   1. Which bindings exist now, per application / environment / kind?
--   2. Which entries in `deploy_app.metadata.integrations` did NOT become a
--      binding, and why? This is the "remaining report" for
--      0013_deploy_application_resource_link_backfill: only parseable rows
--      migrate, so anything listed by query (2) is a row an operator has to fix
--      in the source document (or bind through the API) by hand.
--
-- Nothing here writes, and the queries are independent result sets.

-- ---------------------------------------------------------------------------
-- (1) Bindings per application / environment / kind.
-- ---------------------------------------------------------------------------
SELECT
    a.tenant_id,
    a.uuid                                     AS app_uuid,
    a.name                                     AS app_name,
    link.env_key,
    link.resource_kind,
    count(*)                                   AS binding_count,
    count(*) FILTER (WHERE link.is_default)    AS default_count,
    count(*) FILTER (WHERE link.binding_status = 'DISABLED') AS disabled_count,
    count(*) FILTER (WHERE link.metadata ->> 'source' = 'deploy_app.metadata.integrations')
                                               AS migrated_from_document
FROM deploy_application_resource_link link
JOIN deploy_app a
  ON a.tenant_id = link.tenant_id
 AND a.id = link.app_id
WHERE link.deleted_at IS NULL
  AND a.deleted_at IS NULL
GROUP BY a.tenant_id, a.uuid, a.name, link.env_key, link.resource_kind
ORDER BY a.tenant_id, a.name, link.env_key, link.resource_kind;

-- ---------------------------------------------------------------------------
-- (2) Document entries that did not become a binding, with their reason.
--
-- The reasons mirror the filters in 0013 exactly:
--   unsupported_environment  the environments key is not one of the five
--                            lifecycle values 0012's CHECK accepts
--   invalid_document         the environment value is not an object
--   missing_app_id           officialAccount/miniProgram without `appId`
--   reference_too_long       the reference exceeds resource_ref VARCHAR(128)
--   missing_account_no       a paymentAccounts entry without `accountNo`
--   missing_provider_code    a paymentAccounts entry without `providerCode`
-- ---------------------------------------------------------------------------
WITH documents AS (
    SELECT
        a.tenant_id,
        a.uuid AS app_uuid,
        a.name AS app_name,
        lower(btrim(environment.key)) AS env_key,
        environment.value AS env_doc
    FROM deploy_app a
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
      AND a.metadata -> 'integrations' ->> 'schema' = 'sdkwork.application.integrations/v1'
),
channel_entries AS (
    SELECT d.tenant_id, d.app_uuid, d.app_name, d.env_key,
           'official_account'::text AS resource_kind,
           d.env_doc -> 'officialAccount' AS binding
    FROM documents d
    UNION ALL
    SELECT d.tenant_id, d.app_uuid, d.app_name, d.env_key,
           'mini_program'::text AS resource_kind,
           d.env_doc -> 'miniProgram' AS binding
    FROM documents d
),
payment_entries AS (
    SELECT d.tenant_id, d.app_uuid, d.app_name, d.env_key,
           'payment_account'::text AS resource_kind,
           account.value AS binding
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
entries AS (
    SELECT * FROM channel_entries
    UNION ALL
    SELECT * FROM payment_entries
)
SELECT
    e.tenant_id,
    e.app_uuid,
    e.app_name,
    e.env_key,
    e.resource_kind,
    e.binding ->> 'id'        AS source_account_id,
    coalesce(e.binding ->> 'appId', e.binding ->> 'accountNo') AS source_reference,
    e.binding ->> 'providerCode' AS source_provider_code,
    CASE
        WHEN e.env_key NOT IN ('development', 'test', 'staging', 'demo', 'production')
            THEN 'unsupported_environment'
        WHEN jsonb_typeof(e.binding) <> 'object' THEN 'invalid_document'
        WHEN e.resource_kind = 'payment_account'
             AND btrim(coalesce(e.binding ->> 'accountNo', '')) = '' THEN 'missing_account_no'
        WHEN e.resource_kind = 'payment_account'
             AND btrim(coalesce(e.binding ->> 'providerCode', '')) = '' THEN 'missing_provider_code'
        WHEN e.resource_kind <> 'payment_account'
             AND btrim(coalesce(e.binding ->> 'appId', '')) = '' THEN 'missing_app_id'
        WHEN char_length(btrim(coalesce(e.binding ->> 'appId', e.binding ->> 'accountNo', ''))) > 128
            THEN 'reference_too_long'
        WHEN EXISTS (
            SELECT 1
            FROM deploy_application_resource_link link
            JOIN deploy_app a ON a.tenant_id = link.tenant_id AND a.id = link.app_id
            WHERE a.uuid = e.app_uuid
              AND link.env_key = e.env_key
              AND link.resource_kind = e.resource_kind
              AND link.deleted_at IS NULL
              AND (
                  e.resource_kind <> 'payment_account'
                  OR link.resource_ref = btrim(e.binding ->> 'accountNo')
              )
        ) THEN 'already_bound'
        ELSE NULL
    END AS reason
FROM entries e
ORDER BY e.tenant_id, e.app_name, e.env_key, e.resource_kind, source_reference;

-- ---------------------------------------------------------------------------
-- (3) Mini-program identity: the binding per environment next to the delivery
--     unit's platform identifier.
--
-- These are two different facts (which account the environment publishes as vs
-- which identifier a build is produced under), and they are not constrained
-- against each other: a target may exist before the account is approved. This
-- result set exists so a disagreement is visible instead of silent — compare
-- `bound_mini_program_app_id` with `platform_app_id_value` by hand.
-- ---------------------------------------------------------------------------
SELECT
    a.tenant_id,
    a.uuid        AS app_uuid,
    a.name        AS app_name,
    link.env_key,
    link.resource_ref                 AS bound_mini_program_app_id,
    link.binding_status,
    target.target_key,
    target.platform,
    target.app_id_value               AS platform_app_id_value,
    (target.app_id_value IS NOT NULL
        AND btrim(target.app_id_value) <> ''
        AND btrim(target.app_id_value) <> link.resource_ref) AS values_differ
FROM deploy_application_resource_link link
JOIN deploy_app a
  ON a.tenant_id = link.tenant_id
 AND a.id = link.app_id
 AND a.deleted_at IS NULL
LEFT JOIN deploy_app_platform_target target
  ON target.tenant_id = link.tenant_id
 AND target.app_id = link.app_id
 AND target.deleted_at IS NULL
WHERE link.resource_kind = 'mini_program'
  AND link.deleted_at IS NULL
ORDER BY a.tenant_id, a.name, link.env_key, target.target_key;
