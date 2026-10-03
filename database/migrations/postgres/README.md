# PostgreSQL Migrations

Pre-launch the Deploy schema is consolidated on the single greenfield baseline:
`database/ddl/baseline/postgres/0001_deploy_baseline.sql`. It contains the
complete initial schema (apps, DNS/domain, certificate/TLS lifecycle, node
cluster, upload session refs, artifacts and releases, unified application
delivery — apps, platform targets, source repositories, build templates,
builds, packages, release channels and rollouts, signing identities, usage
metering and entitlement projections, app database profiles, CI source event
ingestion, and the application environment promotion chain).

The baseline carries the complete greenfield schema, and any table added to the
baseline after a schema has been baselined is accompanied by an ordered
`NNNN_*.up.sql` here, so already-initialized schemas converge without a reset.
The lifecycle orchestrator applies the baseline once on an empty schema
(`baseline-plus-migrations`, `lifecycle.autoMigrate=false`) and then applies
these ordered migrations in numeric order; the drift gate verifies the live
schema against `database/contract/`.

`0001_deploy_app_template_marketplace` is the first such migration: it adds the
application template marketplace catalog tables (category, template, version)
for schemas baselined before those tables existed, together with its guarded
`.down.sql`.

After the first production release, keep adding ordered expand/contract
migrations here without rewriting the released baseline; the previous greenfield
migration inventory was folded into the baseline before launch.
