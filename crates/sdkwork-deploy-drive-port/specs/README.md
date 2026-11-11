# SDKWork Deploy Drive Port Specs

`component.spec.json` is the integration contract for the Deploy-to-Drive provider adapter.
Global SDKWork standards remain authoritative through the referenced canonical specs.

## Transport (MODULE_SPEC.md §4.2)

Deploy delegates Drive **upload sessions**: it opens a session, the end user's browser streams the
package bytes into it, and Deploy then asks Drive to finalize or abort it.

- **Default: in-process.** `InProcessDeployDrivePort` runs on the host PostgreSQL pool through
  Drive's own `DriveUploadSessionService` + `DriveObjectRuntimeUploadStorage`. Tenant scoping, the
  `writer`/`reader` ACL, part accounting, the idempotency key, multipart create/complete/abort and
  node-version activation stay inside Drive; the port only translates commands and maps errors.
  No sibling endpoint is read and no extra configuration is required to boot.
- **HTTP is opt-in only.** A real split deployment selects
  `SDKWORK_DEPLOY_DRIVE_MODE=http` together with `SDKWORK_DRIVE_FACADE_URL`; the adapter validates
  its endpoint while the host boots, and it is never selected by the default path.
- **Development** may still select the in-memory port with `SDKWORK_DEPLOY_DRIVE_MODE=memory` or
  the legacy `SDKWORK_DEPLOY_USE_MEMORY_DRIVE=true`; production-like environments reject it.
- The endpoint read sits in `src/drive_http_adapter.rs`, the only module allowed to touch it, and
  is covered by `specs/backend-module-call-boundary.allow.json` for the opt-in split deployment.
