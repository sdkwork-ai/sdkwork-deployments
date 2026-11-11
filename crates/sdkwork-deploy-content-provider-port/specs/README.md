# SDKWork Deploy Content Provider Port

This crate resolves live Drive WebsiteRoot and Knowledgebase WikiPublication identities for the
Deploy module. It never persists provider credentials, source bytes, or raw files.

## Transport (MODULE_SPEC.md §4.2)

- **Default — in-process** (`src/in_process.rs`): Drive website roots are created and re-read
  through `sdkwork-drive-workspace-service` on the host PostgreSQL pool, using Drive's own
  `SqlDrivePermissionService` (`is_space_owner`, `resolve_effective_node_access` + `allows_role`)
  for the `owner` space ACL and `DriveWebsiteRootService::create_root`/`get_root` for the
  idempotent `root_key`/selector upsert. Knowledgebase wiki publications are read from the same
  active-publication row the knowledgebase internal API delegates to
  (`src/knowledgebase_publication.rs`). Drive provider-event registration
  (`src/website_event_delivery.rs`) runs through `DriveWebsiteRootEventDeliveryService` in-process.
- **Opt-in split deployment — HTTP** (`src/content_provider_http_adapter.rs`): only constructed when
  `SDKWORK_DEPLOY_CONTENT_PROVIDER_MODE=http`, and `from_env` fails at startup when an endpoint or
  ingress token file is missing. Every remaining sibling endpoint read lives in that one module and
  is recorded in `specs/backend-module-call-boundary.allow.json`.

`SDKWORK_DEPLOY_USE_MEMORY_CONTENT_PROVIDER=false` now selects the in-process port; `true` keeps
the development in-memory port and is refused in production-like environments.

## Published in-process API

```rust
content_provider_port_from_pool(pool: sqlx::PgPool) -> Arc<dyn ContentProviderPort>
content_provider_port_from_env(pool: &sqlx::PgPool) -> Result<Arc<dyn ContentProviderPort>, String>
website_provider_event_delivery_port_from_env(pool: &sqlx::PgPool)
    -> Result<Arc<dyn WebsiteProviderEventDeliveryPort>, String>

InProcessContentProviderPort::new(pool: sqlx::PgPool)
ValidateContentProviderResourceCommand { tenant_id, organization_id, app_uuid, operator, resource }
ContentProviderActor::user(subject_id)
```
