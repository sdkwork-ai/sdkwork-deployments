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

`apps.identityProvision` re-runs that step for one application; it is the retry
entry an application stuck in `failed` or `pending_identity` uses (see
[Retry a failed or pending application](#retry-a-failed-or-pending-application)).

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

`apps.identityProvision` is the supported retry entry:

```text
POST /backend/v3/api/apps/{appId}/identity_provision
operationId: apps.identityProvision
permission: deploy.apps.write
```

It is a backend (operator) command and it runs the *same* provisioning entry as
`apps.create` (`attempt_app_identity_provisioning`): read the stored status,
replay the row's `identity_provisioning_key`, record the outcome. There is no
second copy of the provisioning logic, so the replay key and the status
transitions cannot drift between the two callers.

1. Resolve `appId` from the report above. It is the application's `uuid`
   (`deploy_app.uuid`); the BIGINT `id` is internal and is not addressable on
   this API.
2. Send the command. There is no request body and no `Idempotency-Key`: the
   application id is the natural key, and two calls for the same application
   are the same installation.
3. Read `data.item` in the response:
   - `identityStatus = active` — the installation exists. When the row was
     already `active` the command called IAM **zero** times: that is the
     intended idempotent short-circuit, not a cached answer. It also does not
     claim a provisioning key, so the `active` + `NULL` key anomaly stays
     visible for triage instead of being quietly repaired.
   - `identityStatus = failed` — the attempt ran and IAM rejected it, timed out,
     or was unreachable. The status is recorded on the row and `updated_at`
     moved; the full error is in the service log with the app id and the
     provisioning key. Fix the cause and retry.
   - `identityStatus = pending_identity` — no provisioning adapter is wired on
     this deployment (`ProvisioningOutcome::Skipped`). Nothing was attempted and
     nothing changed; wire IAM first, then retry. Retrying without wiring it is
     a no-op.
4. The command is audited as `app.identityProvision` with the operator identity
   from the backend token. After a batch of retries, re-run the report to
   confirm the rows moved.

The previous workaround — replaying `apps.create` with the original
`Idempotency-Key` and an identical request body — still works for a create call
that is still reproducible, and it still lands on the same installation because
both paths share the entry point. It is no longer the only way, and it is not
needed for rows created by an internal caller (seeds, provisioning, tests) whose
`idempotency_key IS NULL` cannot be replayed as a create at all.

Do not set `identity_status = 'active'` by hand. That would claim a provisioning
state the IAM side does not have, and the deployment would still fail closed when
it tries to resolve its installation.

### What to check before retrying

- IAM provisioning is configured for this deployment (`ProvisioningOutcome` is
  not `Skipped`). A retry against an unconfigured deployment answers
  `pending_identity` and changes nothing.
- The application is not soft-deleted, and the operator's token carries the
  tenant that owns it.
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
