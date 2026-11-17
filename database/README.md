# DEPLOY Database Module

Canonical lifecycle assets for SDKWork Deploy per `DATABASE_FRAMEWORK_SPEC.md`.

- moduleId: `deploy`
- serviceCode: `DEPLOY`
- tablePrefix: `deploy_`

## Initialization state

This module is in **initialization state** for greenfield deployments:

1. **Baseline** — `database/ddl/baseline/{engine}/0001_deploy_baseline.sql` contains the full DDL snapshot.
2. **Migrations** — `database/migrations/{engine}/` is reserved for post-GA incremental schema changes only. It is intentionally empty at initialization.
3. **Seeds** — every profile includes `common/015_deploy_application_scaffold_categories.sql`, the baseline application-template taxonomy required before the first template create.
4. **Drift** — run `pnpm db:drift:check` before release.

## Identity provisioning state

Migration `0008_deploy_app_identity_provisioning` adds `deploy_app.identity_status` (`pending_identity` | `active` | `failed`) and `deploy_app.identity_provisioning_key`. Use the read-only report [`tools/report_identity_provisioning.sql`](../tools/report_identity_provisioning.sql) to inventory the states and find pending/failed applications and the `active` + NULL-key anomaly. Retry, anomaly triage, and the current entrypoint boundaries are documented in [`docs/runbooks/identity-provisioning-reconciliation.md`](../docs/runbooks/identity-provisioning-reconciliation.md).

## Commands

```bash
pnpm run db:validate
pnpm run db:materialize:contract
pnpm run db:plan
pnpm run db:init
pnpm run db:migrate
pnpm run db:seed
pnpm run db:status
pnpm run db:drift:check
```
