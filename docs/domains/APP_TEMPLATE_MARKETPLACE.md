# App Template Marketplace

What an app template is, how it moves from an author's `deploy_app` into a
purchasable listing, and which surface owns each transition.

This document is the capability companion to the catalog surfaces introduced by
`deploy_app_template_category` / `deploy_app_template` /
`deploy_app_template_version` (database baseline + migration
`0001_deploy_app_template_marketplace`) and the
`templateCategories` / `marketplaceTemplates` / `appTemplates` /
`appTemplateVersions` operation families (app-api + backend-api). The trade
itself — order, payment, entitlement — belongs to the platform order center
(`sdkwork-order` + `sdkwork-payment`) and is documented in that repository's
`specs/APP_TEMPLATE_ORDER_SPEC.md`. This document describes behaviour, not
decisions of record.

## 0. Answering "does deploy_app_template already exist?"

No. Before this capability the only template-shaped table was
`deploy_build_template`, which is a governed **build recipe** (toolchain +
commands for the build runner), not a marketplace listing. The catalog is
greenfield: three tables, two API operation families per surface.

## 1. Entities

| Entity | Table | Owned by | Purpose |
| --- | --- | --- | --- |
| Template category | `deploy_app_template_category` | platform admin | Browsable taxonomy. One optional parent level (`parent_id`), stable `category_key` slug per tenant. |
| App template | `deploy_app_template` | author user | The listing: display copy, media refs, category, visibility, pricing, moderation state. Points at the source `deploy_app` via `app_uuid` (a reference, not an FK — the template outlives optional app deletion semantics). |
| Template version | `deploy_app_template_version` | author user | An immutable published snapshot: `version`, changelog, packaged artifact reference (`artifact_uuid` → `deploy_artifact`), platform-target snapshot, size/checksum. |

There is **no purchase, order or entitlement table in this module**. A settled
`commerce_order` with `subject = 'app_template'` in the order center *is* the
entitlement; `deploy_app_template.install_count` is a counter that center
maintains, not a ledger.

## 2. Lifecycle

Template: `DRAFT → PENDING_REVIEW → PUBLISHED`, with `REJECTED` (back to the
author, with `review_note`) and `DISABLED` (admin kill switch) as side states.
Only `PUBLISHED` + `PUBLIC` templates appear in the marketplace browse surface.

Versions: created `DRAFT`; they become `PUBLISHED` when their template is
approved (the newest approved version becomes `latest_version_uuid`) or, for an
already-`PUBLISHED` template, immediately on create. `WITHDRAWN` removes a
version from acquisition without touching the template.

Acquisition is not a deploy-side lifecycle: the buyer's order moves through the
order center's own states (`pending_payment → paid → fulfilled`), and the
listing's `install_count` advances when that order settles.

## 3. Tenancy and visibility

Everything stays tenant-scoped, exactly like the rest of the module: a template
is visible in the marketplace of **its own tenant only**. Cross-tenant
marketplaces are a future capability that needs an explicit index + policy
change, not an accident of a missing predicate — every browse query filters
`tenant_id = $1` and the repository owner gate follows
`list_domain_zones_repo`. The order center applies the same tenant +
organization scope when it resolves a listing for a purchase.

## 3b. Template types (APP / PPT / VIDEO)

A template records *what kind of artifact it publishes* in
`deploy_app_template.template_type` (`APP`, `PPT`, `VIDEO`; extending the
vocabulary is a baseline CHECK value plus the contract enum, everywhere else —
indexes, queries, clients — treats it as a facet). The rationale: app bundles,
slide decks, and videos are all authored as **project files through
conversational creation**, so one listing table carries them all and the type
is a first-class marketplace facet rather than a different lifecycle. The
column defaults to `APP` (the historical inventory), the marketplace browse
index carries a type-prefixed variant
(`idx_deploy_app_template_marketplace_type`), and every client surfaces the
type filter alongside categories: PC (select facet + create-dialog field),
H5 (type chip row), Flutter (chip row over the generated Dart client), and the
WeChat mini-program (chip row over the wx transport).

## 4. Trade seam: the order center owns the purchase

Money and entitlement never live in the deployments module. Buying a template is
an ordinary order-center trade:

1. **Catalog resolution** — `POST /app/v3/api/app_template_orders`
   (`appTemplateOrders.create`) on the order center resolves the listing from
   `deploy_app_template` (tenant + organization scoped, `PUBLISHED` + `PUBLIC`,
   not authored by the caller) and snapshots its title, price, currency and
   newest published version onto the order item.
2. **Order + payment** — a FREE listing is settled inside the creating
   transaction (order created `paid` + `fulfilled`, install counted once); a
   PAID listing returns the order-bound cashier URL or a provider QR code and
   settles through the PSP webhook pipeline in `sdkwork-payment`.
3. **Fulfillment** — settlement dispatches the `app_template` business type;
   `sdkwork-order-integration-deployments` advances the order's
   `fulfillment_status` and increments `deploy_app_template.install_count` in
   one transaction, counting only the transition that actually moved the order,
   so webhook redelivery and the compensation worker are idempotent.
4. **Read** — `GET /app/v3/api/app_template_orders` (`appTemplateOrders.list`)
   returns the buyer's orders; `status = "paid"` is the install entitlement and
   `versionUuid` is the version it entitles.

Consequences, all enforced by code: this module's API grants nothing — its
marketplace surface is browse + author CRUD; an author cannot buy their own
listing (`conflict`); a second purchase of a listing the buyer already owns
answers with the settled order instead of creating another one.

## 5. Surfaces

| Surface | Operations | Audience |
| --- | --- | --- |
| app-api `/app/v3/api` | `templateCategories.list`; `marketplaceTemplates.list/retrieve`; `appTemplates.list/create/retrieve/update/delete/submit`; `appTemplateVersions.list/create` | every client platform (PC, H5, mini-program, Flutter) |
| backend-api `/backend/v3/api` | `templateCategories.list/create/retrieve/update/delete`; `appTemplates.list/retrieve/update/delete`; `appTemplateVersions.list/retrieve` | publishing control plane (admin CRUD + moderation) |
| order center `/app/v3/api` | `appTemplateOrders.create`; `appTemplateOrders.list` | every client platform, through `@sdkwork/order-app-sdk` |

Permissions derive automatically (`deploy.<family>.read|write`); the PC app
manifest lists them in `backend.accessTokenPermissionScope`. Template orders are
administered through the order center's backend operations (refunds,
cancellations, reconciliation) — this module has no settlement surface.

## 6. Client rollout

The catalog ships as contract + SDK: `@sdkwork/deployments-app-sdk` carries the
`marketplaceTemplates`/`appTemplates`/`appTemplateVersions`/
`templateCategories` clients and `@sdkwork/deployments-backend-sdk` the admin
families. The purchase half is the order center's `appTemplateOrders` family;
this repository owns all four client roots and each one uses the order SDK or
the order app API for buy + entitlement.

| Root | Surface | State |
| --- | --- | --- |
| `apps/sdkwork-deployments-pc` | `sdkwork-deployments-pc-console-marketplace` (browse, detail, acquire, my templates, publish, versions) and `sdkwork-deployments-pc-admin-app-templates` (categories CRUD, moderation) | console pages and admin resource entries registered on the shared workspace |
| `apps/sdkwork-deployments-h5` | `src/marketplace/*` tabs: marketplace + my templates | category/type/search facets, detail sheet, acquire, versions, submit/withdraw |
| `apps/sdkwork-deployments-mini-program` | `pages/marketplace` + `pages/my-templates` | hand-written `wx.request` transport behind a typed port |
| `apps/sdkwork-deployments-flutter` | `lib/marketplace_page.dart` + `lib/marketplace_service.dart` | bottom-nav shell over the generated Dart SDK behind a `MarketplacePort` |

There is no HarmonyOS client root in this repository; a HarmonyOS surface needs
its own application root and `sdkwork.app.config.json` declaration before any
page can be authored.

### 6b. Bootstrap: the first category

Categories are tenant-scoped and platform-admin maintained, so a freshly
provisioned tenant has an empty taxonomy, and `appTemplates.create` requires a
`categoryUuid`. Until the tenant's first category exists, publishing is
impossible. The only path that creates one is the backend surface
(`POST /backend/v3/api/template_categories`, permission
`deploy.templateCategories.write`), driven from the PC admin console's
`templateCategories` resource; the app-api exposes `templateCategories.list`
only. Provision the taxonomy once per tenant before opening the marketplace to
authors.
