# PLAN-2026-0004 Application Ownership And Super-Admin Ledger

Status: proposed
Owner: sdkwork-deploy (entity + contract) / sdkwork-webserver (admin surface)
Related: `TECH-cloud-site-publishing-control-plane.md` §4.2 `deploy_app`,
`DATABASE_SPEC.md` §6.7 Ownership Fields, `API_SPEC.md` §4.2,
`ADR-20260917-dns-credentials-from-iam-cloud-account-center.md`

---

## 0. Outcome First

The admin **应用管理** ledger cannot currently answer two operator questions —
"whose app is this" and "is this a user app or a platform app" — because the
database already stores the ownership dimension but **nothing reads it**, and the
published contract exposes none of it.

Three facts drive every decision below. All are verifiable with the commands in
§7.

1. `deploy_app` **already has** `user_id`, `tenant_id`, `organization_id`,
   `data_scope`, `created_by`, `updated_by`, `activated_at`, `paused_at`,
   `archived_at`, `nginx_conf_sha256` — and the API exposes **none** of them.
2. `deploy_app.user_id` and `deploy_app.data_scope` are **dead columns**: no
   writer, no reader, in the whole repository.
3. The repository's own **already-shipped ownership vocabulary** is
   `platform` / `organization` / `tenant` / `user`
   (`sdkwork-deploy-cloud-account-port/src/lib.rs:52-64`). The requested
   "用户 / 平台应用 / 等" is that vocabulary applied to apps — it does not need to
   be invented.

Recommendation: **Option B** in §2 — an explicit `owner_type` column from the
existing four-level vocabulary, plus an owner gate on `apps.list`. Not a derived
flag, and not a reuse of the legacy `type` integer.

---

## 1. Current-State Audit

### 1.1 What the table stores vs. what the contract exposes

`deploy_app` is created at `database/ddl/baseline/postgres/0001_deploy_baseline.sql:1788`.

| Column (DDL) | Contract `AppResponse` | Consumer |
| --- | --- | --- |
| `id`/`uuid`, `name`, `slug`, `app_kind`, `app_status`, `description`, `default_environment` | exposed | console |
| `platform_target_count` (derived) | exposed | console |
| `app_domain_label`, `app_domain_suffixes` | exposed | console |
| `created_at`, `updated_at`, `version` | exposed | console |
| `type` (INTEGER, `CHECK 1..6`) | **absent** | serialized by DTO as `type`, read by nobody |
| `user_id` | **absent** | **no writer, no reader** |
| `tenant_id` | absent | SQL filter only |
| `organization_id` | absent | written on create, never surfaced |
| `data_scope` | absent | **no writer, no reader** |
| `created_by`, `updated_by` | absent | written on create, never surfaced |
| `activated_at`, `paused_at`, `archived_at` | absent | never written, never surfaced |
| `nginx_conf`, `nginx_conf_sha256` | absent | written by the nginx plane |
| `current_revision_id`, `desired_revision_id` | **absent** | serialized by DTO, no console consumer |
| `deleted_at` | absent | SQL filter only |

### 1.2 The contract is violated in both directions

`apis/app-api/deploy/openapi.yaml` → `components.schemas.AppResponse` is declared
`additionalProperties: false`, and:

- **Declared but never produced**: `siteId` (there is exactly one Rust
  `AppResponse`, at `crates/sdkwork-deploy-contract/src/app_delivery.rs:623`, and
  it has no `site_id` field).
- **Produced but never declared**: `type`, `runtimeConfig`, `currentRevisionId`,
  `desiredRevisionId` — all four are `#[serde(rename = ...)]`-serialized by that
  same struct, so the wire carries them while the contract says no extra
  properties exist. Any generated SDK typed from the contract silently drops them.

This is a live instance of the "declared contract masks the real one" failure
class. It must be resolved as part of this work, because the ownership fields are
added to the very same schema.

### 1.3 Authorization gap (the reason a cosmetic column is not enough)

`list_apps_repo` filters on `tenant_id` **only**
(`crates/sdkwork-intelligence-deploy-repository-sqlx/src/apps.rs:291-312`):

```sql
WHERE a.tenant_id = $1 AND a.deleted_at IS NULL
```

Compare `deploy_dns_zone`, which has an explicit owner gate
(`domain_zones.rs:49-51`):

```rust
fn zone_owner_gate(parameter: usize) -> String {
    format!("(z.user_id IS NULL OR z.user_id = ${parameter})")
}
```

So today **every member of a tenant can list every app in that tenant**,
including other people's personal apps. Adding an `ownerType` column without
adding the matching predicate would produce a column that describes an ownership
model nothing enforces. Per the workspace rule, the predicate change must be
grepped across **all** crates and all variants (`== Some(0)`, `can_cross_tenant`,
…) before landing — the same predicate often lives in both the route and the
service layer.

---

## 2. Ownership Model Selection

### Option A — derive from `user_id` (mirror `ZoneScope`)

`ownerType = user_id IS NULL ? PLATFORM : USER`.

- Zero DDL, one mapping function, proven shape (`ZoneScope::for_owner`).
- Only two values, so `TENANT` and `ORGANIZATION` apps are unrepresentable.
- Semantics ride on a side effect of the column being NULL, and — critically —
  the column is currently NULL for **every** row, so every existing app would be
  classified as a platform app.

**Rejected.** It answers the question asked today and forecloses the next one.

### Option B — explicit `owner_type` + `owner_id` (recommended)

`DATABASE_SPEC.md` §6.7 already mandates this shape: `owner_type` and `owner_id`
represent variable ownership across users, organizations, tenants, apps,
projects, devices, or service accounts; `owner_type` values `MUST` come from a
documented enum; `owner_id` `MUST` be an `int64` subject id; owner-based access
control `MUST` have supporting indexes. §`owner_entity` names the table kind
(`DATABASE_SPEC.md:137`).

Reuse the vocabulary this repository already ships rather than minting a second
one — `sdkwork-deploy-cloud-account-port/src/lib.rs:52-64`:

| `ownerType` | Meaning | `owner_id` | Visibility | Existing constant |
| --- | --- | --- | --- | --- |
| `PLATFORM` | platform-operated / built-in app | `NULL` | every tenant | `ACCOUNT_SCOPE_PLATFORM` |
| `TENANT` | shared across the tenant | `NULL` (tenant is the scope) | all tenant members | `ACCOUNT_SCOPE_TENANT` |
| `ORGANIZATION` | shared inside one organization | `organization_id` | organization members | `ACCOUNT_SCOPE_ORGANIZATION` |
| `USER` | personal app | `user_id` | creator (and platform operators) | `ACCOUNT_SCOPE_USER` |

Widest-first rank already exists (`scope_rank`, `iam.rs:548-555`: user 0 →
organization 1 → tenant 2 → platform 3) and should be reused so list ordering and
precedence stay consistent with the account centre.

### Option C — reuse the legacy `type` integer

`deploy_app.type` is `INTEGER NOT NULL DEFAULT 1 CHECK (type BETWEEN 1 AND 6)`, is
not declared by the contract, and has no consumer. `DATABASE_SPEC.md:225-226`
forbids assigning ad hoc integer meanings, and
`TECH-cloud-site-publishing-control-plane.md` §4.2 already requires the legacy
numeric `site_type`/`status` columns to move by expand-and-contract rather than
silent reinterpretation.

**Rejected.** Reusing a legacy integer for a new, high-traffic authorization
dimension is the exact failure §4.2 warns about.

---

## 3. Field Design

### 3.1 P0 — the ownership plane (the explicit ask)

| Contract field | Type | Source | Notes |
| --- | --- | --- | --- |
| `ownerType` | `AppOwnerType` enum | new column `owner_type` | four values above |
| `ownerUserId` | `string` (int64-as-string) | `user_id` | present only for `USER` |
| `ownerId` | `string` | `user_id` / `organization_id` / `null` | resolved subject id |
| `tenantId` | `string` | `tenant_id` | cross-tenant location, platform-operator only |
| `organizationId` | `string \| null` | `organization_id` | `0`/`NULL` must read as "none", not as an org |

### 3.2 P0 — the visibility leak this exposes

Adding the column obliges the predicate (§1.3). The list query gains:

```sql
AND ($n = '' OR ($n = 'USER'         AND a.owner_type = 'USER'         AND a.user_id = $owner)
            OR ($n = 'ORGANIZATION' AND a.owner_type = 'ORGANIZATION' AND a.organization_id = $org)
            OR ($n = 'TENANT'       AND a.owner_type = 'TENANT')
            OR ($n = 'PLATFORM'     AND a.owner_type = 'PLATFORM'))
```

plus the reachability gate itself (`owner_type IN ('PLATFORM','TENANT') OR
owner matches`), exactly as `zone_owner_gate` separates *reach* from *scope
facet*. Absent `scope` must keep today's answer for callers that never send it.

### 3.3 P1 — super-admin ledger fields, professionally motivated

These are the fields a platform operator needs that the current ledger cannot
answer. Ordered by what breaks operationally if absent.

| # | Field | Source | Why an operator needs it |
| --- | --- | --- | --- |
| 1 | `currentRevisionId` / `desiredRevisionId` | already in DDL + DTO | `desired != current` is **an un-applied release** — the single most useful drift signal, already computed and thrown away |
| 2 | `lastDeployedAt` | `deploy_deployment.completed_at` (max) | `latestReleaseTag` says *what*, never *when* |
| 3 | `activatedAt` / `pausedAt` / `archivedAt` | already in DDL, never written | status badge shows the current state, never how long it has held it |
| 4 | `createdBy` / `updatedBy` | already in DDL + written | who changed this, the first question in every incident |
| 5 | `healthStatus` | `deploy_*health_check` rows | aggregate pass/fail per app |
| 6 | `certificateExpiryAt` | certificate lifecycle | silent outage source; "expires in 9 days" belongs on the ledger |
| 7 | `customDomainCount` | `deploy_domain` under the app | separates platform-derived hostnames from operator-claimed ones |
| 8 | `nginxConfigOverridden` | `nginx_conf_sha256 IS NOT NULL` | who overrode the generated sidecar config |
| 9 | `databaseProfileCount` / `environmentCount` | related tables | data-plane and environment footprint at a glance |
| 10 | `usageLast30d` | `deploy_app_usage_daily` | cost/quota reconciliation entry point |
| 11 | `dataScope` | already in DDL | **conditional**: either implement it or document it as reserved. Exposing a constant `1` as if it carried meaning is worse than omitting it |

### 3.4 Deliberately not added

| Candidate | Why not |
| --- | --- |
| `ownerDisplayName` on `AppResponse` | This database has **no user table** (`CREATE TABLE .*user` has no match in the baseline; `user_id` has no FK). A denormalized name snapshot drifts on rename, and `DATABASE_SPEC.md` only permits such a snapshot as a read-model projection. The repository precedent is to expose the **id**: `ProviderAccountResponse.ownerUserId`. Resolve names in the console against the host's identity read |
| `type` (1..6) surfaced as an "application type" column | Collides with the existing `appKind` column, whose label is already `应用类型`; showing a legacy integer under a competing name misleads |
| Billing amounts inline | Belongs on the usage surface, not a list row |

### 3.5 The "用户" column, resolved

Show `ownerUserId` as the cell, with the display name resolved by the host when
an identity read is available and the raw id otherwise. This keeps the
`deploy_app` entity free of a user directory it does not own, and matches how
`ownerUserId` is already exposed for provider accounts.

---

## 4. Landing Plan

Cross-repo, five layers. Each row is a required stop; skipping one produces the
"implemented but not wired" pattern this workspace already documents.

**sdkwork-deployments — entity and contract**

1. `database/ddl/baseline/postgres/0001_deploy_baseline.sql` — add
   `owner_type VARCHAR(16) NOT NULL DEFAULT 'USER'` + `CHECK` on the four
   documented values; add `idx_deploy_app_owner (tenant_id, owner_type, user_id)`
   (required by §6.7 "MUST have supporting indexes").
2. ~~`database/migrations/postgres/` — one migration~~ **Corrected while landing.**
   `database/migrations/postgres/README.md` states that pre-launch the schema is
   consolidated on the single greenfield baseline and that **no ordered
   post-baseline migrations exist**; shared development schemas converge by
   resetting the module state to the baseline rather than replaying forward-only
   migrations. Adding a migration file here would be the deviation, not the fix.
   The column therefore lands **in the baseline**, and the regeneration step is
   `pnpm db:materialize:contract` followed by `pnpm db:validate`.
3. `crates/sdkwork-deploy-contract/src/app_delivery.rs` — `AppOwnerType` enum;
   `AppResponse` gains the §3.1/§3.3 fields; `CreateAppRequest` gains an optional
   `ownerType`; `UpdateAppRequest` gains a double-`Option` `ownerType` (present /
   null / absent must stay distinguishable, per the existing `appDomainLabel`
   precedent); `ListAppsQuery` gains `scope` + `keyword`.
4. `crates/sdkwork-intelligence-deploy-repository-sqlx/src/apps.rs` — `APP_SELECT`
   + `INSERT` + `map_app_row` + the list predicate and its owner gate.
5. `crates/sdkwork-intelligence-deploy-service/src/app.rs` — thread the owner
   subject from `DeployAppRequestContext`; stop discarding `actor_id`.
6. `crates/sdkwork-routes-deploy-app-api/src/app_delivery_routes.rs` — accept the
   query parameters. Note: the served `/openapi.json` drops authored parameters
   from `.list` routes, so the **handler's `Query<T>` is the path that actually
   takes effect** — the YAML alone changes nothing.
7. `apis/app-api/deploy/openapi.yaml` **and** the generated
   `deploy-app-api.openapi.json` — add the fields, delete `siteId`, and close the
   four-field divergence in §1.2.
8. `sdks/` — regenerate. The manual `.d.mts` must be updated in the same change
   or cross-repo typecheck goes red downstream.

**sdkwork-webserver — admin surface**

9. `apps/sdkwork-webserver-pc/packages/sdkwork-webserver-pc-console-publishing`
   (source of truth in `sdkwork-deployments`) — ledger columns, `scope` facet
   filter, and the `PublishingAppsPage` column list.
10. `src/i18n.ts` — `columnOwner`, `columnOwnerType`, plus the `ownerType` enum
    label map in **both** `en` and `zh` (the catalog is a
    `Record<keyof typeof en, string>`, so asymmetry is a compile error).
11. `apps/sdkwork-webserver-pc/tests/applications-operations-column.test.tsx` —
    the host-side gate for the bridged ledger must keep passing; extend it to
    assert the two new columns render for a `PLATFORM` row **and** a `USER` row,
    since the failure mode is "one surface wired, the other not".
12. Confirm both hosts of `PublishingAppsPage` —
    `sdkwork-deployments-pc/src/App.tsx` and
    `webserver-pc-console-delivery/src/DeployAppsManagementSurface.tsx` — are
    covered. The page takes its clients as props, so only the adapter changes.

### Column-width budget

The operations column is pinned `min-width:144px` in the host
`deploy-surface.css` and already carries 6-7 icon actions; the ledger already has
8 columns. Two more must either displace low-value columns (`slug`, `version`) or
move behind the detail drawer. Verify by **measuring each `<th>`'s
`getBoundingClientRect().width`** — `scrollWidth` reports a false green.

---

## 5. Schema Change And Backfill

`owner_type` cannot default to `USER`: every existing row carries
`user_id IS NULL`, so `DEFAULT 'USER'` would produce rows that claim a user owner
and have none — and the paired constraint below would reject them outright.

### 5.1 As landed in the baseline

- `owner_type VARCHAR(16) NOT NULL DEFAULT 'TENANT'` — the honest reading of this
  table's own history: the list query has always been tenant-wide.
- `chk_deploy_app_owner_type` — the four documented values.
- `chk_deploy_app_owner_pointer` — the level and its pointer must agree:
  `USER` ⇒ `user_id IS NOT NULL`; `ORGANIZATION` ⇒ `organization_id <> 0`;
  `PLATFORM`/`TENANT` ⇒ `user_id IS NULL`. This is what stops a row from
  advertising `USER` with no user (an empty owner cell that reads as missing data
  rather than a broken write).
- `idx_deploy_app_owner (tenant_id, owner_type, user_id)` and
  `idx_deploy_app_owner_organization (tenant_id, organization_id)`, both partial
  on `deleted_at IS NULL` — `DATABASE_SPEC.md` §6.7 requires owner predicates to
  have supporting indexes; the organization leg is separate because a
  `(tenant_id, owner_type, user_id)` scan cannot serve it.
- `user_id` gained the comment distinguishing it from `created_by`: the owner can
  be transferred, the command issuer cannot.

### 5.2 Backfill for an existing schema

Converging a shared development schema is a reset to the baseline, so this
backfill matters only for a schema that carries real data:

1. Add the column as `DEFAULT 'TENANT'` and add the two indexes.
2. Rows where a user owner is actually known:
   `UPDATE deploy_app SET owner_type='USER', user_id=created_by WHERE created_by IS NOT NULL AND deleted_at IS NULL`.
   (`created_by` *is* populated on create; `user_id` is not — `apps.rs:251`
   inserts `created_by`/`updated_by` from `actor_id`, while `user_id` is absent
   from the column list entirely.)
3. Add `chk_deploy_app_owner_pointer` **after** step 2, or the update is rejected
   mid-flight.
4. Rows with no `created_by` (seeds, internal provisioning) stay `TENANT`.

An app the operator intends as platform-operated must be moved to `PLATFORM`
explicitly — it is not inferable from existing data.

### 5.3 Cutover

`apps.create` starts writing `owner_type` and `user_id`; `apps.list` starts
honouring the owner gate. Both land in the same change: a column with no
predicate is decoration, and a predicate with no writer locks everyone out of
their own apps.

---

## 6. Verification

- Contract/implementation parity: diff the serialized `AppResponse` from a live
  `GET /app/v3/api/apps/{appId}` against the authored schema, field by field,
  both directions. This is the gate that would have caught §1.2.
- Ownership predicate: real-database test per `DATABASE_SPEC` — a `USER`-owned app
  created by A must not be listed for B in the same tenant, while a `PLATFORM`
  app must be listed for both. Fixtures that pass only because of the gap must be
  identified, not trusted.
- Both hosts: render the ledger with a `PLATFORM` row and a `USER` row and assert
  the columns; confirm only one adapter changed.
- Any new gate must be mutation-tested (control group green first, then restore
  and re-check the hash) — an always-green assertion proves nothing.

---

## 7. Reproducing The Audit

```sh
cd D:/sdkwork-space/sdkwork-deployments

# 1. dead columns: no writer, no reader
grep -rn "user_id\|data_scope" crates/sdkwork-intelligence-deploy-repository-sqlx/src/apps.rs   # → empty
grep -rn "data_scope" database/ --include=*.sql                                                  # → DDL only

# 2. contract vs DTO divergence
grep -rn "pub struct AppResponse" -A 45 crates/sdkwork-deploy-contract/src/app_delivery.rs
python -c "import json;print(json.load(open('apis/app-api/deploy/deploy-app-api.openapi.json'))['components']['schemas']['AppResponse']['properties'].keys())"

# 3. the ownership vocabulary already in the repo
sed -n '52,64p' crates/sdkwork-deploy-cloud-account-port/src/lib.rs
sed -n '545,555p' crates/sdkwork-deploy-cloud-account-port/src/iam.rs

# 4. the missing owner gate
sed -n '291,312p' crates/sdkwork-intelligence-deploy-repository-sqlx/src/apps.rs
sed -n '49,51p'  crates/sdkwork-intelligence-deploy-repository-sqlx/src/domain_zones.rs
```

---

## 8. Open Questions

1. Is `TENANT` the correct default for the ~existing rows, or should an operator
   bulk-declare a subset as `PLATFORM` at cutover?
2. Does a super-admin view need cross-tenant listing (`tenant_id` omitted), which
   requires a separate authorization gate rather than a filter value?
3. Is `data_scope` implemented now (per-app visibility inside a tenant) or
   documented as reserved? Exposing it as a constant is not an option.
4. Name resolution for the 用户 column: host-side identity read, or id-only for
   this release.
