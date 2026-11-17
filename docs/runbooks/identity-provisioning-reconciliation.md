# Identity Provisioning Reconciliation

Status: active
Owner: SDKWork Deploy maintainers
Related migration: `0008_deploy_app_identity_provisioning`
Report tool: [`tools/report_identity_provisioning.sql`](../../tools/report_identity_provisioning.sql)

## State model

Migration 0008 gives every deployment application an identity-provisioning
state:

| Column | Meaning |
| --- | --- |
| `deploy_app.identity_status` | `pending_identity` \| `active` \| `failed` |
| `deploy_app.identity_provisioning_key` | idempotency key sent to IAM; `deploy:{id}` once claimed |

`apps.create` provisions the IAM installation as its last step. The call is
deliberately infallible from the caller's point of view: the `deploy_app` row is
already committed, a cross-module failure is recorded as `failed`, and the
application is not deleted or rolled back. `ProvisioningOutcome::Skipped` (IAM
provisioning not configured on this deployment) leaves the state
`pending_identity` until an operator wires the integration.

## Run the report

The SQL is read-only: every data statement is a `SELECT`, and the file wraps
them in `BEGIN; SET TRANSACTION READ ONLY; ... COMMIT;` so an accidental write
fails instead of mutating the inventory.

```bash
psql "$SDKWORK_DATABASE_URL" -v ON_ERROR_STOP=1 \
  -f tools/report_identity_provisioning.sql
```

It returns three result sets:

1. **Inventory by state** — `identity_status` and `application_count`.
2. **Pending and failed applications** — `id`, `uuid`, `tenant_id`, `slug`,
   `identity_status`, `identity_provisioning_key`, `created_at`, `updated_at`,
   ordered oldest-first. This is the actionable set.
3. **Anomaly: active but no key** — an application that reports `active` while
   `identity_provisioning_key IS NULL`. The normal path claims the key *before*
   the IAM call (`begin_app_identity_provisioning`), so such a row cannot have
   come from the create path. Report it for investigation; do not silently
   normalise it.

All queries exclude `deleted_at IS NOT NULL` rows: a soft-deleted application
is not an open reconciliation item. To scope to one tenant, append
`AND tenant_id = <tenant_id>` to the relevant `WHERE` clause.

## Retry a failed or pending application

The only provisioning trigger that exists today is `apps.create`
(`DeployAppService::create_app` → `provision_app_identity`). There is **no**
update-triggered or rebuild-triggered provisioning path, and there is **no**
`requeue` command in this repository as of this change. Do not assume either
exists; a requeue command would be a new entrypoint that must be documented
when it is implemented.

The supported retry is an idempotent replay of the original create:

1. Read the application row to recover its identity: `uuid` is the application
   id, and the original `Idempotency-Key` is the one the create command used
   (`deploy_app.idempotency_key`).
2. Re-send `apps.create` with the **same** `Idempotency-Key` and an identical
   request body (the body is hashed into `request_sha256`; a different body with
   the same key is rejected as a conflict).
3. The idempotent create returns the existing application instead of inserting a
   new one, and `create_app` then calls `provision_app_identity` again.
   `begin_app_identity_provisioning` reuses the stored
   `identity_provisioning_key` (`COALESCE(identity_provisioning_key,
   'deploy:' || id::text)`), so the retry cannot create a second IAM
   installation. The call is skipped only when the status is already `active`.
4. Watch the row: `active` means the IAM installation was provisioned;
   `failed` means it was attempted and rejected (the error is logged by the
   service, and `updated_at` moves); `pending_identity` with a `Skipped`
   outcome means IAM provisioning is not configured on this deployment.

Rows created by internal callers (seeds, provisioning, tests) may have
`idempotency_key IS NULL`. For those rows the create-replay retry is not
available, and no alternative requeue entrypoint exists as of this change; do
not fabricate one in an operator procedure.

Do not set `identity_status = 'active'` by hand. That would claim a provisioning
state the IAM side does not have, and the deployment would still fail closed when
it tries to resolve its installation.

### What to check before retrying

- IAM provisioning is configured for this deployment (`ProvisioningOutcome` is
  not `Skipped`).
- The application is not soft-deleted.
- The IAM side is reachable and its `iam_tenant_application` unique indexes are
  satisfied (`deploy_application_id`, `provisioning_idempotency_key`).

## Anomaly triage: active with a NULL key

The report's third query lists these rows. Because the key is claimed before the
IAM call, an `active` row with no key indicates data written outside the normal
path (manual SQL, an older revision, or a restore). Treat it as a finding:

1. Confirm the IAM installation exists and is bound to this application's
   `deploy_app.id` (the IAM backfill runbook owns that link).
2. Record the finding before changing anything; the reconciliation report is
   evidence.
3. Do not invent a key retroactively as a "repair": the key is an idempotency
   token, and fabricating one does not prove which IAM call produced the state.

## Related runbook

The IAM side of the same relation — linking legacy installations to their
deployment application — is owned by `sdkwork-iam`:
`docs/runbooks/backfill-installation-deploy-link.md`.
