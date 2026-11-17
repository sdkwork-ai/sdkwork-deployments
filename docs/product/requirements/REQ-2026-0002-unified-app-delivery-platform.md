# REQ-2026-0002 Unified App Delivery Platform

```yaml
id: REQ-2026-0002
title: Deliver static web, API services, mini-programs, and mobile/HarmonyOS applications from source through one versioned control plane
owner: SDKWork Deploy maintainers
status: ready
source: product
problem: The current Deploy control plane models only web Sites with live Drive/Wiki sources. Customers cannot bind a source repository, compile application source into standardized deployment packages, or manage immutable semantic versions, channels, review submissions, and rollbacks for API services, WeChat/Douyin mini-programs, iOS/Android applications, and HarmonyOS applications.
users:
  - application developers
  - tenant application administrators
  - mini-program publishers
  - mobile release managers
  - platform build operators
  - platform administrators
goals:
  - model every deployable product as a tenant App with one or more platform targets (web, API, WeChat, Douyin, iOS, Android, HarmonyOS)
  - bind Git source repositories with secret-reference credentials and commit traceability
  - compile source through governed build templates into standardized immutable deployment packages
  - manage immutable semantic versions with channels, promotion, lifecycle, retention, and full source-to-runtime traceability
  - deploy through typed targets: Web runtime sets, containers, mini-program review submission, store/TestFlight/OTA channels
non_goals:
  - execute arbitrary customer server code or ungoverned build commands
  - store repository credentials, signing keys, keystores, or upload secrets in the database
  - replace platform-owned review processes (WeChat review, App Store review)
  - move price books, invoices, payments, or taxes into Deploy
  - move source repository hosting or container registry byte ownership into Deploy
affected_surfaces:
  - api
  - sdk
  - backend
  - database
  - pc
  - deployment
  - security
  - observability
```

Specs: REQUIREMENTS_SPEC.md, ARCHITECTURE_DECISION_SPEC.md, DOMAIN_SPEC.md, DATABASE_SPEC.md,
API_SPEC.md, SDK_SPEC.md, APP_SDK_INTEGRATION_SPEC.md, CONFIG_SPEC.md, DEPLOYMENT_SPEC.md,
SECURITY_SPEC.md, PRIVACY_SPEC.md, PERFORMANCE_SPEC.md, OBSERVABILITY_SPEC.md, TEST_SPEC.md,
RELEASE_SPEC.md, MIGRATION_SPEC.md

## Current Deficiencies

1. **Web-only top-level model.** `deploy_app.site_kind` covers only `STATIC`, `SPA`, `WIKI`, and
   `HYBRID`. There is no tenant `App` aggregate and no platform-target abstraction, so one business
   application cannot express "same source, multiple platforms" (for example Flutter producing iOS
   and Android, or an H5 web plus a WeChat mini-program sharing a codebase).
2. **No source repository model.** `deploy_deployment.deploy_type=2 (Git)` and the free-text
   `source_ref`/`commit_hash` columns are legacy fields without repository binding, credential
   custody, branch policy, or immutable commit snapshots.
3. **No build pipeline.** There is no build task state machine, no toolchain/build template
   (Node, Flutter, Xcode, AGP, hvigor, mini-program CI), no build log capture, no artifact
   collection, and no quality gates.
4. **No deployment package standard.** `deploy_artifact.package_type` is an ad hoc integer (1-5);
   there is no package format taxonomy, no in-package manifest standard, no per-format platform
   validation (WeChat 2 MiB main package, Android `minSdk`/signature, iOS bundle identifier/signing,
   HarmonyOS `apiVersion`), no signing identity, and no provenance chain.
5. **No version management.** `deploy_release.version_tag` is free text; there is no semantic
   version contract, no `(app, target, version)` uniqueness, no channels (stable/beta/alpha/gray),
   no version lifecycle, no per-environment mapping, and no
   source-commit -> build -> package -> release -> deployment traceability chain.
6. **Single deployment target class.** Only Web-node assignment and generic upload sessions exist.
   Mini-program review submission, App Store/TestFlight, enterprise OTA, container/Kubernetes
   rollout, and percentage gray rollout are absent.
7. **Signing and secret custody not extended to builds.** iOS code signing, Android keystores,
   HarmonyOS certificate profiles, and mini-program upload keys have no model; the secret-reference
   custody principle already applied to TLS keys must extend to repository credentials and signing
   identities.
8. **No build/package metering and quota.** Build minutes, package storage, version retention, and
   channel counts have no entitlement or usage dimensions.
9. **Legacy model coexistence.** `deploy_deployment` (deploy types 1-4, status 0-5) coexists with
   the SiteRevision/artifact model without convergence rules.

## Functional Requirements

1. `deploy_app` shall be the tenant-owned application aggregate. One application is
   one **endpoint**: H5, PC, mini-program, Flutter, iOS, Android, HarmonyOS, API, and
   desktop applications are separate `deploy_app` rows, not variants of one row.
   Supported `app_kind` values are
   `STATIC_WEB`, `SPA_WEB`, `H5_WEB`, `PC_WEB`, `API_SERVICE`, `WECHAT_MINIPROGRAM`,
   `DOUYIN_MINIPROGRAM`, `IOS_APP`, `ANDROID_APP`, `HARMONYOS_APP`, `FLUTTER_APP`, and
   `DESKTOP_APP`. `STATIC_WEB`/`SPA_WEB` are the legacy rendering-model kinds;
   `H5_WEB`/`PC_WEB` are the endpoint-accurate web kinds; `FLUTTER_APP` is the
   cross-platform kind whose platform targets name the stores it ships to. A value
   outside this set is rejected by the database.

   Each application belongs to exactly one ownership mode:
   `TENANT` (`organization_id = 0`, the default when no organization is named),
   `ORGANIZATION` (`organization_id` = the owning organization), or `USER`
   (`owner_user_id` = the owning person, `organization_id = 0`). `PLATFORM` is the
   platform-internal fourth level and is never selectable through a tenant command.
   The modes are mutually exclusive in the database, and `apps.list` can filter by
   mode, organization, and endpoint kind.
2. Every App shall own one or more `deploy_app_platform_target` rows. A platform target carries the
   platform (`WEB`, `API`, `WECHAT`, `DOUYIN`, `IOS`, `ANDROID`, `HARMONYOS`), the tech stack
   (`FLUTTER`, `NATIVE`, `UNI_APP`, or web/API stacks), the platform identity (bundle id, package
   name, app id, bundle name), and its allowed delivery channels. One source repository may feed
   multiple platform targets.
3. An App whose delivery is a web Site shall link its `deploy_app` configuration; existing Sites
   keep working without an explicit App and get an implicit App of kind `STATIC_WEB`/`SPA_WEB`.
4. `deploy_source_repository` shall bind a Git repository (URL, provider, default branch, clone
   policy) to an App. Repository credentials are stored only as opaque secret references; Deploy
   never persists tokens or private keys.
5. `deploy_build_template` shall define the governed build recipe per platform target: toolchain
   contract (versions and environment), bounded command list, artifact output paths, and quality
   gates. Templates are validated and versioned; arbitrary shell escape is prohibited.
6. `deploy_build` shall record one build execution per (App, platform target) with a strictly
   monotonic `build_number`, an immutable source snapshot (commit SHA, branch/tag, message, author),
   a bounded state machine (`QUEUED`, `PREPARING`, `COMPILING`, `TESTING`, `PACKAGING`,
   `SUCCEEDED`, `FAILED`, `CANCELLED`, `TIMED_OUT`), a Drive-backed log reference, and produced
   package references. Retried executions reuse the claimed build row; `build_number` never
   decreases.
7. `deploy_package` shall be the immutable deployment package. Each package records the package
   format, the standard manifest digest, byte size and checksum, opaque Drive storage references,
   the producing build, the signing identity, and platform requirement metadata. Package records
   are never mutated after acceptance.
8. The deployment package standard `sdkwork.deploy-package.v1` shall define the in-package
   manifest and per-format validation rules:
   - `WEB_STATIC`/`WEB_SPA`: bounded directory bundle with index entry and no secrets;
   - `API_SERVICE`: immutable container image reference or a process bundle (binary plus entry
     contract); no live-registry writes from Deploy;
   - `WECHAT_MINIPROGRAM`/`DOUYIN_MINIPROGRAM`: archive with platform manifest entry and platform
     size ceilings;
   - `IOS_APP`: `.ipa`/`.xcarchive` with bundle identifier, minimum iOS version, and signing
     identity requirements;
   - `ANDROID_APP`: `.apk`/`.aab` with package name, `minSdk`/`targetSdk`, ABI set, and signature
     verification requirement;
   - `HARMONYOS_APP`: `.hap`/`.app` with bundle name, API version, and signing profile
     requirement.
9. `deploy_release` shall reference exactly one immutable package and carry a semantic version
   (SemVer 2.0.0 with bounded build metadata). `(tenant, app, platform_target, semantic_version)`
   shall be unique. Releases are immutable; version lifecycle states are `DRAFT`, `ACTIVE`,
   `SUPERSEDED`, `DEPRECATED`, `RETIRED`, and `ARCHIVED`.
10. `deploy_release_channel` shall maintain the current release pointer per channel key
    (`stable`, `beta`, `alpha`, `qa`) per (App, platform target), and `deploy_channel_rollout`
    shall record immutable promotion/assignment history including strategy (immediate, percentage
    gray rollout, manual approval), so every channel transition is auditable and reversible.
11. `deploy_deployment` shall be extended with deployment kinds for `ARTIFACT_RELEASE`,
    `SITE_CONFIG`, `TLS_CONFIG`, `MINIPROGRAM_REVIEW`, `STORE_SUBMISSION`, `OTA_DISTRIBUTION`,
    `ENTERPRISE_DISTRIBUTION`, and `CONTAINER_ROLLOUT`, plus target, strategy, platform review
    reference, percentage, and rollback linkage. Existing numeric legacy values remain readable.
12. Every new mutation shall write `deploy_audit_log`; every package, build, and release record
    shall carry tenant scope and optimistic `version` per DATABASE_SPEC.md. Rollback is re-release
    of a prior immutable release, never mutation of history.
13. Build minutes, package storage bytes, version/channel counts, and deployment counts shall feed
    `deploy_usage_event` and the entitlement projection; retention policies shall cover build logs,
    packages, releases, and rollout history.
14. Cross-repository calls (repository fetch, Drive upload of packages and logs, platform upload
    commands) shall use owner-generated SDKs or approved service ports with SDKWork authentication;
    raw HTTP and manual auth headers are not accepted.

## Acceptance Criteria

- A tenant can create an App of each `app_kind`, attach platform targets, bind a Git repository,
  and see an immutable source snapshot captured on every build.
- A build runs through the bounded state machine, captures a Drive-backed log, and either produces
  one validated `deploy_package` with a standard manifest or fails with a stable error code.
- Each package format is rejected when its platform validation rules fail (size ceiling, missing
  platform manifest, mismatched bundle identity, missing signing identity).
- Two releases of the same (App, platform target, semantic version) cannot both exist; a lower
  `build_number` can never supersede a higher one.
- Channel promotion records immutable rollout history and the channel current pointer changes only
  through a new assignment.
- A deployment to any supported target class records start, platform review reference (where the
  platform has review), completion, and rollback linkage; audit rows exist for every transition.
- The full trace source commit -> build -> package -> release -> channel assignment -> deployment
  resolves in one bounded query without exposing secrets.
- The legacy `deploy_deployment`, `deploy_artifact`, and `deploy_app` rows remain readable and
  compatible; no destructive migration is introduced.
