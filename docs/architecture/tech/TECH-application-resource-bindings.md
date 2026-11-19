# Application Account Bindings: Resource Link Authority Matrix

Status: active
Owner: SDKWork Deploy maintainers
Updated: 2026-11-17
Specs: DATABASE_SPEC.md, API_SPEC.md, SDK_SPEC.md, APPLICATION_IDENTITY_SPEC.md, TEST_SPEC.md

## 1. The Problem

An application that is *published* on WeChat does not only consist of a bundle. A production
environment serves through an official account, a mini program, and one or more payment provider
accounts, and each of those is issued per environment: a production app must never inherit a
development official account or a sandbox merchant account.

Three places in the Deploy schema can name such an account, and before this document they had no
stated owner:

| Place | What it actually records |
| --- | --- |
| `deploy_app.metadata.integrations` (`sdkwork.application.integrations/v1`) | The console's nested binding document: `linkedApplications` plus one entry per environment with `officialAccount`, `miniProgram` and `paymentAccounts[]`. |
| `deploy_application_resource_link` | A normalized binding row: `(app, env_key, resource_kind, resource_ref)` with `provider_code`, `is_default`, `binding_status`, `metadata`. |
| `deploy_app_platform_target.app_id_value` | The platform-side identifier of one **delivery unit** inside the app (the Android `applicationId`, the iOS bundle id, the WeChat mini program AppID a build is uploaded with). |

`deploy_application_resource_link` shipped in the baseline with zero readers and zero writers, so the
document was the *de facto* authority and the table was dead weight. The platform target was never an
authority for account selection; it is the build/release identity.

## 2. Authority Matrix

| Fact | System of record | Reader(s) | Never written by |
| --- | --- | --- | --- |
| Which official account an app environment represents | `deploy_application_resource_link` (`resource_kind = 'official_account'`) | app-api, runtime resolution, console | platform target, legacy document |
| Which mini program an app environment represents | `deploy_application_resource_link` (`resource_kind = 'mini_program'`) | app-api, runtime resolution, console | platform target, legacy document |
| Which payment accounts an app environment can settle through, and which one is the default per provider | `deploy_application_resource_link` (`resource_kind = 'payment_account'`) | app-api, payment routing, console | platform target, legacy document |
| Which platform targets an app delivers to, and the platform-side identifier of each delivery unit | `deploy_app_platform_target` (`platform`, `target_key`, `app_id_value`, …) | build/package/release/channel APIs, build runner | resource link |
| The cross-system anchor (`linkedApplications`) and the pre-migration binding document | `deploy_app.metadata.integrations` | console (anchor), migration/reconciliation (bindings) | account-binding writers |
| Display snapshot / provider facts a binding was approved with | `deploy_application_resource_link.metadata` | console | any derived reader |

Rules:

1. **Account bindings are owned by `deploy_application_resource_link`.** Every read of "which account
   does this environment act as" goes through it. No new code reads bindings out of
   `deploy_app.metadata`.
2. **Deployment targets are owned by `deploy_app_platform_target`.** A target answers "which platform
   does this app deliver to and under which platform-side identifier is the artifact produced". It
   never answers "which account does the published environment act as".
3. **`deploy_app.metadata.integrations` is a legacy input, not a binding authority.** Its
   `environments` half is read exactly once by the ordered migration
   `0013_deploy_application_resource_link_backfill` and thereafter only by the reconciliation report
   and the console's migration seed; it is never rewritten. The `linkedApplications` half remains the
   cross-system anchor and is the only part the console writes.
4. **No component writes two of them.** The three writers are disjoint: the resource-link API writes
   only `deploy_application_resource_link`; the platform-target API writes only
   `deploy_app_platform_target`; the backfill writes only bindings that do not exist yet and never
   touches the source document.

## 3. `resource_link` vs `platform_target.app_id_value`

Both can hold a WeChat AppID, and that overlap is the only realistic double-write risk. They answer
different questions, and the split is deliberate:

| Question | Answering table | Used at |
| --- | --- | --- |
| Which WeChat AppID does this environment publish/preview as, and with which provider credentials? | `deploy_application_resource_link.resource_ref` (`mini_program`) | runtime resolution, channel authorization, console binding drawer |
| Which WeChat AppID is the *build* uploaded under, and which target does a package/release/channel hang off? | `deploy_app_platform_target.app_id_value` | build creation, package registration, release/channel promotion |

Therefore:

- **`platform_target` is not a read-only projection of `resource_link`.** It carries facts the binding
  does not have (platform, tech stack, bundle id, package name, allowed channels, build template) and
  it must exist even when no account is bound yet (a build can be produced before the account is
  approved).
- **`resource_link` is not derived from `platform_target`.** An environment may bind a mini program
  before any target exists, and may bind an account that no target references.
- **They must agree when both exist.** The console writes both from one form field where the two
  overlap (WeChat mini program AppID), and `tools/report_app_resource_links.sql` prints them side by
  side (query 3) so an inconsistency is visible instead of hidden. A disagreement is a defect in the
  console write path, not a state the runtime resolves by preference: publishing reads the resource
  link, the build reads the target, and the operator is told.
- **The single-instance rule keeps the runtime deterministic.** `uk_deploy_application_resource_link_single`
  allows at most one `official_account` and one `mini_program` row per `(app, env_key)`, so there is no
  "which binding wins" question at read time.

## 4. Schema Contract

`deploy_application_resource_link` (see `database/migrations/postgres/0012_*` for the reconciliation):

| Column | Contract |
| --- | --- |
| `id` | The binding's public identifier (uuid string); the API's `id`. |
| `tenant_id`, `organization_id`, `app_id` | Ownership and the application the binding hangs off. |
| `env_key` | `development \| test \| staging \| demo \| production` — enforced by `chk_deploy_application_resource_link_environment`, identical to every other environment-scoped deploy table. |
| `resource_kind` | `official_account \| mini_program \| payment_account`. |
| `resource_ref` | The provider-side identity: WeChat AppID for a channel account, merchant account number for a payment account. Non-blank, ≤ 128 characters. |
| `provider_code` | `wechat`, `alipay`, `stripe`, … Required for `payment_account` (the default is unique per provider), optional for a channel account. |
| `is_default` | Payment only, at most one per `(app, env_key, provider_code)`. Always `FALSE` for the single-instance kinds. |
| `binding_status` | `ACTIVE \| DISABLED`. A disabled binding keeps its row and its audit trail. |
| `metadata` | Display snapshot and provider facts (account name, mode, settlement currency) plus, for migrated rows, the provenance (`source`, `sourceSchema`, `sourceAccountId`). |

Uniqueness:

- `uk_deploy_application_resource_link_ref` — one row per `(app, env_key, kind, resource_ref)`.
- `uk_deploy_application_resource_link_single` — one `official_account`/`mini_program` per environment.
- `uk_deploy_application_resource_link_default` — one default payment account per
  `(app, env_key, COALESCE(provider_code, ''))`. The `COALESCE` closes the NULL hole in the baseline
  definition: PostgreSQL treats NULLs as distinct in a unique index, so `(app, env, NULL)` could appear
  arbitrarily many times while still claiming to be "the default".

### 4.1 Why `id` is not the module's `BIGINT id + uuid` pair

Every other deploy table carries an internal `BIGINT id` plus an external `uuid`. This table's `id`
*is* the external identifier (a uuid string), and it is deliberately unchanged:

- `DATABASE_SPEC.md` §6.1 forbids ad-hoc id allocation (`MAX(id)+1`, sequences, `DEFAULT nextval`,
  random hashes) — including from a migration. Adding a `BIGINT id` now would require
  `0013_deploy_application_resource_link_backfill` to mint ids in SQL for rows that only exist inside a
  JSON document.
- `DATABASE_SPEC.md` §6.3 allows a backfill to preserve existing stable ids; the source document
  already carries a stable account id (`officialAccount.id`, `paymentAccounts[].id`), and the binding's
  own id is derived deterministically from its address.
- Nothing references the column: there is no foreign key into `deploy_application_resource_link`, so the
  internal/external split buys no referential power today, and the API already exposes the uuid.

Revisit when the first external table needs a foreign key into a binding: at that point the conversion
is a real schema change with real callers, and it can be reviewed on its own evidence rather than
smuggled into a backfill.

## 5. API Surface

`apps.resourceLinks.list` / `apps.resourceLinks.update` / `apps.resourceLinks.delete` on
`/app/v3/api/apps/{appId}/resource_links`.

- **The write is a `PUT` and an explicit upsert** (API_SPEC §15.4: "upsert behavior MUST be explicit
  and must not be hidden behind a normal create operation"). The request addresses one binding:
  - a single-instance kind is addressed by `(environment, resourceKind)`, so switching an environment's
    official account is one call and cannot leave the environment with two;
  - a payment account is addressed by `(environment, resourceKind, resourceRef)`, so binding another
    merchant account adds a row.
- **Repeating a request is a no-op on state.** The address is the idempotency key, which is why the
  operation declares no `Idempotency-Key` requirement.
- **The default moves.** Setting `isDefault` on a payment account clears it from the previously default
  account of the same provider in the same transaction; the unique index is never violated and the
  caller is never asked to clear the old default first.
- **Deletion is soft.** `DELETE` retires the row (`deleted_at`, `version + 1`) so the audit trail keeps
  answering "which account used to serve production".
- **Reach.** Every operation runs the credential's application-scope gate
  (`APPLICATION_IDENTITY_SPEC.md` §4.1) before the owner gate and before any row is touched, so a token
  bound to application A cannot read, rebind, or retire application B's bindings.

## 6. Data Migration And Reconciliation

`0013_deploy_application_resource_link_backfill` normalizes every
`sdkwork.application.integrations/v1` document into bindings:

| Document entry | Becomes |
| --- | --- |
| `environments.<env>.officialAccount` | `official_account`, `resource_ref = appId`, `provider_code = providerCode` (optional) |
| `environments.<env>.miniProgram` | `mini_program`, same rules |
| `environments.<env>.paymentAccounts[]` | `payment_account`, `resource_ref = accountNo`, `provider_code = providerCode` (required), `is_default = isDefault` (first per provider wins) |

Properties, each covered by `tests/app_resource_links_postgres.rs`:

- **Idempotent and re-runnable** — the binding id is derived deterministically from
  `(tenant, app, env, kind, reference)`, and the insert is guarded by the natural key plus
  `ON CONFLICT DO NOTHING`. Running the file twice inserts nothing the second time; a binding written
  through the API after the first run is never overwritten.
- **Only parseable rows migrate** — an entry without `appId` / `accountNo` / `providerCode`, or under an
  environment key that is not one of the five lifecycle values, stays in the document untouched.
- **The source document is never rewritten** — it is the evidence that makes the normal form
  re-derivable.
- **The default slot stays unique** — a document that lists two defaults for one provider still
  produces one default binding; the other account is bound and reported as non-default.
- **Rollback is exact** — the down migration deletes only rows whose `metadata.source` is the
  backfill's marker.

`tools/report_app_resource_links.sql` is the read-only reconciliation report:

1. bindings per app / environment / kind, including how many came from the document;
2. every document entry that did **not** become a binding, with a reason
   (`missing_app_id`, `missing_account_no`, `missing_provider_code`, `unsupported_environment`,
   `reference_too_long`, `invalid_document`, `already_bound`);
3. the mini-program binding of every environment next to each platform target's `app_id_value`, with a
   `values_differ` flag (see section 3).

Run it after `db:migrate`, and after any change to the integrations document, until the console writes
through the API instead.

## 7. Verification

```bash
cargo test -p sdkwork-deploy-contract --lib          # binding validation and normalization
cargo test -p sdkwork-intelligence-deploy-repository-sqlx --test app_scope_gates
SDKWORK_DATABASE_TEST_POSTGRES_URL=... \
  cargo test -p sdkwork-intelligence-deploy-repository-sqlx \
  --test app_resource_links_postgres -- --ignored --test-threads=1
cargo test -p sdkwork-routes-deploy-app-api --test app_surface_manifest
node <sdkwork-specs>/tools/check-api-operation-patterns.mjs --workspace .
node <sdkwork-specs>/tools/check-api-response-envelope.mjs --workspace .
```

## 8. Open Items

- **The console write path is migrated (2026-11-19).** `sdkwork-cloudrouter`'s admin
  Application Center now reads and writes bindings through
  `apps.resourceLinks.list|update|delete`; it writes only `linkedApplications` into
  `deploy_app.metadata.integrations` and reads the legacy `environments` document once as a
  migration seed, leaving it untouched. The reconciliation report remains useful until every
  tenant has opened and saved the drawer once.
- **`platform_target.app_id_value` is not validated against the mini program binding.** The
  consistency check is a report query, not a constraint: the two rows can legitimately be created at
  different times, so a hard constraint would make "create the target, then bind the account" illegal.
- **Anonymous cashier WeChat OAuth is not application-scoped end to end.** The cashier capability
  token carries the tenant and order but no deploy application id, so the IAM payment OAuth start and
  callback resolve the tenant-wide WeChat integration. A tenant with several official accounts
  should mint an app-scoped cashier token before relying on anonymous JSAPI payment; the
  signed-in cashier path is app-scoped. This spans `sdkwork-utils-rust` (claims), the order/payment
  minting calls, and IAM, so it is tracked as its own change.
- **No runtime consumer reads bindings yet.** The table is the authority as of this change, but the
  publishing path still resolves accounts elsewhere; wiring it is a separate change with its own
  evidence.
