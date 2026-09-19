# SDKWork Deploy PRD

Status: active
Owner: SDKWork maintainers
Application: sdkwork-deploy
Updated: 2026-07-30
Specs: REQUIREMENTS_SPEC.md, DOCUMENTATION_SPEC.md

## Document Map

- [PRD-cloud-site-publishing-platform.md](PRD-cloud-site-publishing-platform.md) - commercial live
  Drive Space-root/folder directory and every-Knowledgebase Wiki publishing product, user console,
  admin console, frozen artifact/release workflows, domains, client Variants, TLS, metering, and
  launch gates.
- [PRD-unified-app-delivery-platform.md](PRD-unified-app-delivery-platform.md) - unified tenant App
  delivery for static web, API services, WeChat/Douyin mini-programs, iOS/Android (Flutter and
  native), and HarmonyOS: source repositories, governed builds, the deployment package standard,
  semantic version channels, and typed deployment targets.
- [Standards alignment](../../standards-alignment.md)
- [Technical standards alignment](../../architecture/tech/TECH-standards-alignment.md)

Deprecated redirect:

- [PRD-2026-06-14-deploy-webserver-prd.md](PRD-2026-06-14-deploy-webserver-prd.md) - retained only
  as a redirect to the current product authority; it contains no active requirements.

## 1. Background And Problem

Product detail lives in the active cloud publishing shard above. It is the product authority for
live directory/Wiki Sites and frozen artifact/release deployment. Live WebsiteRoot and
WikiPublication content changes do not enter the artifact/release pipeline.

## 2. Deployments PC Product Surface

`apps/sdkwork-deployments-pc` is the runnable PC management application. Its tenant Console covers
sites, environment configuration, root-domain Zones and hostname details, managed-certificate
lifecycle metadata, package artifacts,
releases, deployments, and monitoring. Its separately loaded backend-admin surface exposes only
the Nginx, server, and audit operations currently defined by the Deploy Backend API.

Application package bytes are uploaded through `@sdkwork/drive-app-sdk`. Deploy receives stable
Drive upload, space, and node references through `artifacts.create`, then owns immutable artifact,
release, deployment, and runtime-assignment business state. Disabling an application is the
recoverable `apps.pause` command; re-enabling it is `apps.activate`; retiring it is `apps.delete`
(which archives the application rather than hard-deleting its history).

Console packages must consume `@sdkwork/deployments-app-sdk` and `@sdkwork/drive-app-sdk` through
console-core. Backend-admin packages must consume `@sdkwork/deployments-backend-sdk` through the lazy
admin-core boundary. UI packages must not construct clients, issue raw HTTP, create authentication
headers, or treat presigned URLs and provider object keys as business identity.

The domain inventory lists root-domain Zones first. Opening a Zone navigates to a dedicated hostname
management page with verification, binding, certificate coverage, pause, and guarded-delete
operations. Site workspaces associate existing verified hostnames through bindings; hostname rows
do not own a Site. A Site supports multiple hostnames and a hostname may be covered by multiple
certificate aggregates.

Custom private-key ingestion remains disabled until an approved Secret Manager/KMS provider is
configured. There is no Drive-backed private-key path. Production deployments use externally
terminated TLS until distribution, activation, and served-certificate evidence meet the commercial
release gate.


## 9. Open Questions

- Which approved KMS/Secret Manager provider owns one-time custom private-key ingestion after the
  managed-domain/TLS ADR is accepted?
- Which backend-admin rollout and forced-disable operations will be added after their audit,
  recovery, expiry, and authorization contracts are approved?
