# Deploy API Changelog

This file records breaking and compatibility-relevant changes to the Deploy
app-api and backend-api authorities. Deploy is pre-launch (`releaseState:
not_published` in both SDK manifests), so a breaking change is landed directly
without a compatibility alias; the approval is recorded here rather than
carried as a retired alias.

## 2026-11-26 — Certificates: arbitrary hostnames, DNS-01 presentation, archive read model (additive + reconciled)

The certificate surface had drifted from the router: three operations the route
crate already serves were absent from the authority, and two request/response
fields the service already produced were absent from its schemas. This change
closes the drift and adds the operator-facing pieces the console needs.

**Reconciled operations** (present in the router, now in the authority):

| Operation | Method and path | Schema |
| --- | --- | --- |
| `certificates.refresh` | `POST /app/v3/api/certificates/{certificateId}/refresh` | `data.item` = `CertificateResponse` |
| `certificates.challenges.list` | `GET /app/v3/api/certificates/{certificateId}/challenges` | `data.item` = `CertificateChallengePresentationList` |
| `certificates.challenge.verify` | `POST /app/v3/api/certificates/{certificateId}/challenge/verify` | `data.item` = `CertificateChallengeReleaseResult` |
| `certificates.archive.retry` | `POST /app/v3/api/certificates/{certificateId}/archive/retry` | `data.item` = `CertificateArchiveRetryResult` |

`certificates.refresh` is read-only despite the `POST`: it re-reads the
aggregate so the console can ask for a fresh snapshot without implying that a
renewal was requested. `certificates.challenge.verify` rejects with `409`
carrying `CERTIFICATE_DNS_RECORD_NOT_VISIBLE` when the configured resolvers
actually saw the record absent; an unverifiable lookup (nobody answered) does
not block, because blocking there would make manual mode unusable on a
deployment with no trusted resolver. Both `verify` and `archive.retry` answer
`200` with `accepted: false` when there is nothing left to do — a retry, not a
failure — which is the same precedent `CertificateChallengeReleaseResult` set.

**`CreateCertificateRequest`** gains two fields:

- `requestedIdentifiers: string[]` — identifier hostnames supplied directly
  instead of by claim id, which is what a free-form hostname field produces.
  Every submitted name is canonicalized (lower-case, no trailing dot, at most a
  single leading wildcard label), then resolved against the tenant's existing
  active and verified hostname claims through the same zone-owner gate
  `domainIds` already goes through, so the ownership check is identical and only
  the input form differs. A name that is not a verified claim of this tenant is
  refused naming the offending hostname.
- `challengePresentation: MANAGED | MANUAL` (default `MANAGED`) — who presents
  the challenge response. `MANUAL` returns the exact TXT records and waits for
  an operator to publish them before validation is released. Orthogonal to
  `validationMethod`: DNS-01 is legitimate in both modes, while `MANUAL` with
  `HTTP_01` is refused because the edge webroot is not operator-addressable.

`domainIds` is no longer required and is now `maxItems: 8` (the product
identifier ceiling), because it and `requestedIdentifiers` are alternatives:
exactly one must be provided, and sending both is a validation error
(`CERTIFICATE_IDENTIFIER_SOURCE_CONFLICT`) rather than a union or intersection.
Existing clients that send `domainIds` alone are unaffected.

**`CertificateResponse`** gains:

- `presentationMode: MANAGED | MANUAL` (required, always serialized) — both
  modes look identical in `status`, and only this field says whether the
  operator has a record to publish. Rows written before the switch read as
  `MANAGED`, which is what their issuance was.
- `archiveStatus: NONE | PENDING | STORING | ARCHIVED | FAILED | SKIPPED`
  (required) — whether the issued material has been archived to Drive.
  `SKIPPED` is a deployment with archiving switched off and must not be
  rendered as a failure; `NONE` is a certificate that was never issued.
- `archiveSpaceId` and `archiveFolderNodeId` — opaque Drive identifiers (not
  numeric ids) naming where the archive lives, present once it exists.

Generated TypeScript and Flutter app SDKs must be regenerated from the updated
authority; the route manifest and `http_route_manifest.rs` were re-materialized
from it in this change.

## 2026-11-21 — App identity provision echoes the decimal application id (additive)

`POST /backend/v3/api/apps/{appId}/identity_provision` now also answers
`deployApplicationId`: the application's decimal `deploy_app.id`, serialized as
an int64 string. Registering an installation therefore returns the same IAM
anchor that `apps.create`, `apps.retrieve`, and `apps.update` already publish,
instead of forcing the caller to derive it from the uuid. The generated
TypeScript backend SDK was regenerated from the authority.

The app list's `keyword` facet now also matches that decimal id (alongside the
app name and slug), so the id the console prints can be pasted into the list
search. The generated app and backend SDKs were regenerated from the updated
authority descriptions.

## 2026-11-21 — Application-scaffold surface rename (breaking)

The physical catalog has been `deploy_application_scaffold*` since the
application-factory restructure, but the HTTP surface still used the retired
app-template vocabulary. This change aligns the wire contract with the
database and the `APPLICATION_FACTORY_DEPLOYMENT_SPEC.md` scaffold concept.

| Before | After |
| --- | --- |
| `GET/POST /app/v3/api/app_templates` | `GET/POST /app/v3/api/scaffolds` |
| `/app/v3/api/app_templates/{templateUuid}` and `/submit` | `/app/v3/api/scaffolds/{scaffoldUuid}` and `/submit` |
| `/app/v3/api/app_templates/{templateUuid}/versions` | `/app/v3/api/scaffolds/{scaffoldUuid}/versions` |
| `/app/v3/api/template_categories` | `/app/v3/api/scaffold_categories` |
| `/app/v3/api/marketplace/templates` | `/app/v3/api/marketplace/scaffolds` |
| `/app/v3/api/marketplace/templates/{templateUuid}/install` | `/app/v3/api/marketplace/scaffolds/{scaffoldUuid}/install` |
| `/backend/v3/api/app_templates*` | `/backend/v3/api/scaffolds*` |
| `/backend/v3/api/template_categories*` | `/backend/v3/api/scaffold_categories*` |
| `appTemplates.*` | `scaffolds.*` |
| `appTemplateVersions.*` | `scaffolds.versions.*` |
| `templateCategories.*` | `scaffolds.categories.*` |
| `marketplaceTemplates.*` | `marketplace.scaffolds.*` |
| `deploy.appTemplates.read\|write` | `deploy.scaffolds.read\|write` |
| `deploy.appTemplateVersions.read\|write` | `deploy.scaffolds.read\|write` |
| `deploy.templateCategories.read\|write` | `deploy.scaffoldCategories.read\|write` |
| `deploy.marketplaceTemplates.read` | `deploy.marketplaceScaffolds.read` |

Preserved contracts:

- `deploy_application_scaffold` database column names (`template_key`,
  `template_type`, `template_version`, `template_id`) are the design-time
  stable identity named by `APPLICATION_FACTORY_DEPLOYMENT_SPEC.md` §2; the
  wire fields `templateKey`, `templateType`, and `templateVersion` mirror them.
- The order-center subject `app_template` and the `app.template.install` audit
  action are commerce/audit identifiers owned by the order center; they are not
  part of the catalog surface and are unchanged.
- Generated TypeScript and Flutter SDKs were regenerated from the new
  authorities; every in-repo consumer (PC console, H5 marketplace, mini
  program, agent skills) was moved in the same change.
