# Deploy API Changelog

This file records breaking and compatibility-relevant changes to the Deploy
app-api and backend-api authorities. Deploy is pre-launch (`releaseState:
not_published` in both SDK manifests), so a breaking change is landed directly
without a compatibility alias; the approval is recorded here rather than
carried as a retired alias.

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
