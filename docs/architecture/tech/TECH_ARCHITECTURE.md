# SDKWork Deploy Technical Architecture

Status: active
Owner: SDKWork maintainers
Updated: 2026-07-30
Specs: ARCHITECTURE_DECISION_SPEC.md, DOCUMENTATION_SPEC.md, DOMAIN_SPEC.md, DATABASE_SPEC.md,
DEPLOYMENT_SPEC.md, SECURITY_SPEC.md

## Document Map

- [TECH-standards-alignment.md](TECH-standards-alignment.md)
- [TECH-cloud-site-publishing-control-plane.md](TECH-cloud-site-publishing-control-plane.md) - active
  cloud publishing ownership, implemented composition/SDK/runtime-assignment foundation, target TLS
  lifecycle, remaining production gates, and verification matrix.
- [TECH-unified-app-delivery-platform.md](TECH-unified-app-delivery-platform.md) - active unified
  tenant App delivery model: application types, source repositories, governed builds, the
  `sdkwork.deploy-package.v1` deployment package standard, semantic version management with
  channels, and typed deployment targets.
- [TECH-application-resource-bindings.md](TECH-application-resource-bindings.md) - active account
  binding authority: `deploy_application_resource_link` owns official account / mini program /
  payment account bindings, `deploy_app_platform_target` owns deployment targets and platform-side
  identifiers, and `deploy_app.metadata.integrations` is the legacy input the ordered backfill
  normalizes once.

## 1. Architecture Overview

Architecture detail lives in the linked TECH shards above. The cloud publishing architecture is the
  active implementation boundary. Its App composition/provider/descriptor/desired-assignment path,
  cloud single-writer process isolation, Drive/Wiki delivery, provider-event processing,
  authenticated runtime observations, immutable convergence evidence, strict all-target quorum, and
  current-revision advancement are implemented. The Deployments PC Console/backend-admin host and
  Drive-backed artifact registration flow are implemented. External public probes, cloud TLS
  provider automation, metering, and production evidence remain gated by the linked ADR, prelaunch
  convergence record, and readiness review.


## 2. Technology Choices

## 3. System Boundaries And Modules

```text
apps/sdkwork-deployments-pc
  |-- console-* -> console-core -> Deploy App SDK + Drive App SDK
  |-- admin-* -> lazy admin-core -> Deploy Backend SDK
  `-- root bootstrap -> IAM + one TokenManager + typed browser runtime config

File -> Drive uploader -> stable Drive references -> artifact -> release -> deployment
```

Drive owns byte upload, storage attribution, retention, and cleanup. Deploy owns the immutable
publishing graph and never stores presigned URLs, provider object keys, certificate private-key
content, or browser credentials.

## 4. Directory And Package Layout

The PC root uses `sdkwork-deployments-pc-*`, `sdkwork-deployments-pc-console-*`, and
`sdkwork-deployments-pc-admin-*` packages. Console and admin capability packages depend on their
surface core only through injected service/SDK contracts and do not import each other's business
implementation.

## 5. API, SDK, And Data Ownership

- Console remote operations use `@sdkwork/deployments-app-sdk`; package bytes use
  `@sdkwork/drive-app-sdk`.
- Backend-admin operations use `@sdkwork/deployments-backend-sdk` and are not exposed through the
  tenant Console.
- `artifacts.create` accepts stable Drive references and is idempotent. Release and deployment
  creation reference immutable Deploy identities rather than upload URLs.
- Site pause is the reversible application-disable semantic; destructive deletion remains a
  separate confirmed operation.
- `deploy_dns_zone` owns root-domain inventory; `deploy_domain` owns verified hostnames;
  `deploy_app_binding` is the only Site/hostname association.
- `deploy_certificate_identifier` provides certificate/hostname many-to-many coverage, and
  `deploy_listener_certificate_binding` permits parallel RSA/ECDSA versions per listener.

## 6. Security, Privacy, And Observability

The root creates one TokenManager shared by IAM and authenticated SDK clients. Public runtime
configuration contains only non-secret base URLs and profile metadata. TLS private-key custody
remains outside Drive, ordinary SQL columns, API responses, and browsers. Secret Manager/KMS
absence fails closed; only opaque immutable bundle references may cross the control-plane boundary.

## 7. Deployment And Runtime Topology

## 8. Architecture Decision Index

- [ADR-20260721 Unified Cloud Site Publishing Control Plane](../decisions/ADR-20260721-unified-cloud-site-publishing-control-plane.md)
- [ADR-20260723 Managed Domain And TLS Control Plane](../decisions/ADR-20260723-managed-domain-tls-control-plane.md) - accepted; production enablement remains evidence-gated.

## 9. Verification

Implementation status and framework integration: [TECH-standards-alignment.md](TECH-standards-alignment.md).

```powershell
pnpm verify
```
