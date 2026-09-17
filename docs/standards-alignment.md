# Standards Alignment

SDKWork Deploy standards alignment for `sdkwork-deployments`, updated 2026-07-23.

## Integrated Frameworks

| Framework or contract | Status | Evidence |
| --- | --- | --- |
| `sdkwork-web-framework` | Integrated | Auth layers, route manifests, `WebRequestContext`, success/problem response mapping |
| `sdkwork-database` | Integrated | One PostgreSQL/SQLite database contract, lifecycle host, materialization and drift validation |
| `sdkwork-utils-rust` | Integrated | API envelopes, pagination, parsing, hashing, and shared utilities |
| Deploy App/Backend SDK families | Generated and buildable | Owner-only sdkgen inputs, family manifests, composed TypeScript facades, generated transports |
| Drive App/Internal SDKs | Integrated | WebsiteRoot create/reuse, exact Internal revalidation, Node-scoped event-channel registration/renewal, and Drive-backed artifact uploads |
| Knowledgebase Internal SDK | Integrated | Exact ACTIVE canonical WikiPublication validation and bounded capability projection |
| Web Internal SDK | Integrated | Immutable runtime-set publication with per-attempt ingress-token-file loading |
| `sdkwork-discovery` | Deferred by topology | HTTP-only application gateway; required when RPC services are introduced |

## Live Composition Contract

The active mutation is `apps.composition.update` on
`PUT /app/v3/api/apps/{appId}/composition`. It requires dual-token authentication,
`deploy.sites.write`, `If-Match`, and `Idempotency-Key`.

Provider calls complete before database locking. PostgreSQL and SQLite then use the same atomic
sequence: idempotency replay check, tenant Site lock/version check, target validation, normalized
composition replacement, descriptor compilation, immutable SiteRevision insert,
`desired_revision_id` update, complete runtime assignment insert, replay result, and audit commit.
The Site version is a decimal string. `current_revision_id` is reserved for verified Web
observation/quorum and is not advanced by the composition transaction.

Ordinary Drive and Knowledgebase content changes never call this mutation and do not create
`deploy_release`, `deploy_deployment`, or `deploy_app_revision` records.
The runtime worker registers and renews referenced Drive WebsiteRoot channels through the generated
Drive Internal SDK before Web publication. Drive then delivers ordinary events directly to the
Node-qualified Web callback; Deploy is not the event relay or acknowledgement authority.

Domain management starts with `domain_zones`, each owned by a user subject (`user_id`);
`user_id IS NULL` marks platform tenant-level zones. Opening a Zone lists its apex and child
hostname resources; application association exists only through `deploy_app_binding`, so one Site
can use multiple hostnames and one verified hostname can participate in multiple non-conflicting
application routes. `domainZones.hostnames.verify` exposes an expiring DNS TXT challenge at
`_sdkwork-verification.<hostname>`; only an observed current token can atomically verify the
hostname. Missing records, mismatches, resolver failures, and stale tokens fail closed. Periodic
revalidation, wildcard-overlap claim serialization, takeover holds, and production DNS incident
evidence remain launch gates.

## API And SDK Contract

All App and Backend handlers use SDKWork v3:

- success: `{ "code": 0, "data": { "item" | "items" + "pageInfo" }, "traceId": "..." }`;
- error: HTTP 4xx/5xx `application/problem+json` with numeric `code` and `traceId`;
- owner OpenAPI under `apis/`, materialized authority and deterministic `*.sdkgen.json` under the
  owning family, generated transport under `generated/server-openapi`;
- consumers import only `@sdkwork/deployments-app-sdk` or explicit backend-admin
  `@sdkwork/deployments-backend-sdk`.

Backend composition mutation is intentionally absent until an approved trusted operator
credential-delegation or resolved-resource contract exists.

## Runtime And Secret Configuration

Production uses `SDKWORK_DEPLOY_USE_MEMORY_CONTENT_PROVIDER=false` and explicit Drive,
Knowledgebase, and Web Internal API URLs. Ingress credentials are read from projected files under
`/run/secrets/sdkwork/`; values are not stored in environment variables or runtime descriptors.
Drive/Knowledgebase files are read per provider request and the Web file per publication attempt,
which supports atomic rotation without restart.

## Artifact Pipeline Boundary

Drive upload sessions, `deploy_artifact`, `deploy_release`, and `deploy_deployment` remain valid for
Git, package, image, and frozen-bundle workflows. They are not the publication mechanism for a live
WebsiteRoot or WikiPublication. Certificate and private-key upload sessions are not part of the
Drive artifact pipeline. Certificate material is represented only by immutable Secret Manager/KMS
bundle references and is never accepted through a Drive node reference.

## Unified App Delivery Platform

The unified application delivery model (REQ-2026-0002, ADR-20260804) is implemented on the
control plane:

- `deploy_app` is the tenant application aggregate (STATIC_WEB, SPA_WEB, API_SERVICE,
  WECHAT_MINIPROGRAM, DOUYIN_MINIPROGRAM, IOS_APP, ANDROID_APP, HARMONYOS_APP, DESKTOP_APP) with
  `deploy_app_platform_target`, `deploy_source_repository`, `deploy_build_template`,
  `deploy_build`, `deploy_package`, `deploy_release` (semver unique), `deploy_release_channel`,
  `deploy_channel_rollout`, and `deploy_signing_identity` tables from migration 0007.
- Desktop and operating-system delivery is implemented: `DESKTOP_APP` targets
  `WINDOWS`/`MACOS`/`LINUX` with the installer format matrix (MSI/NSIS/MSIX/EXE, DMG/PKG,
  DEB/RPM/AppImage) validated at the container boundary with a 2 GiB ceiling, JVM artifacts
  (JAR/WAR) for the API platform, `ELECTRON`/`TAURI` tech stacks, `WINDOWS_AUTHENTICODE` /
  `MACOS_DEVELOPER_ID` signing identities, `MICROSOFT_STORE` / `MAC_APP_STORE` targets, and
  desktop auto-update manifests (Electron `latest.yml`, Tauri `latest.json`, Sparkle
  `appcast.xml`) with SHA-512 checksum binding.
- CI event ingestion is implemented (migration 0010): GitHub-compatible webhook
  ingestion at `/backend/v3/api/source_events` with `X-Hub-Signature-256` HMAC verification
  (secret via `SDKWORK_DEPLOY_WEBHOOK_SECRET`, endpoint fails closed without it), per-commit
  deduplication, and automatic build triggering for active targets on the default branch.
- The application environment model is implemented (migration 0010):
  `deploy_app_environment` (env key/level/approval requirement/current release pointer) with
  chain-enforced promotion (`fromEnvironmentId` must hold the release) and immutable
  `deploy_environment_promotion` history via `/app/v3/api/apps/{appId}/environments`.
- The application database structure contract is implemented (migration 0009):
  `deploy_app_database_profile` (engine/catalog/schema contract per app) and
  `deploy_app_database_migration` (versioned migration definitions with SHA-256 checksum
  binding to releases); API surface
  `/app/v3/api/apps/{appId}/database_profiles[/{profileId}[/migrations[/{migrationId}]]]`.
- Usage metering is implemented (migration 0008): append-only `deploy_usage_event` facts with
  tenant-scoped deduplication idempotency (`build_minutes` on terminal builds,
  `package_storage_bytes` on package registration, `deployment_count` on deployment creation,
  emitted fire-and-warn), the Commerce-backed `deploy_tenant_entitlement_projection` read model,
  and the reconcilable `deploy_app_usage_daily` aggregate. The tenant read surface is
  `GET /app/v3/api/usage_events`; the service layer and repository integration tests cover
  dedup replay, tenant scoping, and pagination.
- Semantic versioning (SemVer 2.0.0), monotonic `build_number` per (App, platform target),
  (app, target, version) uniqueness, channel promotion with immutable rollout history, and the
  deployment kinds for mini-program review, store submission, OTA/enterprise distribution, and
  container rollout are implemented with typed validation in `sdkwork-deploy-core`.
- The `sdkwork-deploy-package-validator` crate enforces the `sdkwork.deploy-package.v1` byte
  boundary: ZIP/TAR_GZ/DIST_DIR scanning with traversal/symlink/size rejection and platform
  ceilings (WeChat/Douyin main and total packages).
- The `sdkwork-deploy-build-runner` crate owns the executor boundary: claim loop, bounded command
  execution, platform command constructors, OTA manifest generators, and the review-observation
  boundary (`ReviewObserver`). Real signing and platform upload adapters remain gated on
  credential integration.
- The Deployments PC core packages expose the standard `./sdk`, `./modules`, `./host`,
  `./session`, and `./composition` surface with module-catalog permission inheritance.

## Verification

```powershell
pnpm db:validate
pnpm api:materialize
pnpm api:check
pnpm sdk:generate
node ../sdkwork-specs/tools/check-sdk-standard.mjs --workspace .
node ../sdkwork-specs/tools/check-component-port-bindings.mjs --root . --strict
cargo test -p sdkwork-deploy-runtime-compiler --test knowledgebase_wiki_delivery_contract
cargo test --workspace --offline
```

The focused cross-repository contract test compiles a real Deploy Site and runtime set, activates
the exact bytes in Web Server, executes host/path/device routing through the Knowledgebase provider
adapter and a fake generated-SDK boundary, fails private/unpublished routes closed, and observes a
live content update without changing the SiteRevision, runtime-set generation, or snapshot hash.

## Remaining Product Gates

These are explicit launch scope, not hidden compatibility debt. Authenticated Web
observation/quorum, current-revision advancement, Drive/Wiki live reads, and direct provider-event
processing are implemented. The compiler-to-Wiki execution contract is also verified locally;
production-shaped evidence remains required:

- external public-domain probes, drift dashboards, and multi-node rollout/rollback drills;
- production capacity/event-storm evidence for the implemented bounded provider metadata cache,
  including private revocation, negative TTL, stale policy, and multi-node freshness drills;
- certificate secret custody, and ACME hot-activate/SNI verification (the TLS control
  plane is implemented: ACME accounts, certificate order/challenge state machines, and
  transactional certificate version storage via `/backend/v3/api/tls/*`; the RFC 8555 ACME
  client boundary and HTTP-01 verification endpoint remain credential/network-gated);
- tenant console, platform admin console, abuse, and incident workflows (metering facts are
  emitted, entitlement enforcement is implemented behind the
  `SDKWORK_DEPLOY_ENTITLEMENT_ENFORCEMENT` switch, retention enforcement runs via
  `/backend/v3/api/retention/run` with `SDKWORK_DEPLOY_RETENTION_*_DAYS` windows, and the
  daily usage aggregate reconciles idempotently via `/backend/v3/api/usage/reconcile`;
  Commerce projection ingestion remains external);
- continuous topology evidence that cloud Web workloads cannot activate standalone management authority;
- production PostgreSQL backup/restore, multi-node rollout, load, security, and recovery evidence;
- governed publication of the already generated App/Backend SDK packages;
- credential integration for real platform executors: WeChat/Douyin review observation, App Store
  Connect/TestFlight submission, iOS/Android/HarmonyOS signing, and OTA/enterprise distribution
  adapters that replace the no-op review observer and command executors;
- Drive-backed build log and package byte registration in the runner (currently bounded
  local log references).
