# App Template Marketplace

What an app template is, how it moves from an author's `deploy_app` into a
purchasable listing, and which surface owns each transition.

This document is the capability companion to the contract surfaces introduced by
`deploy_app_template_category` / `deploy_app_template` /
`deploy_app_template_version` / `deploy_app_template_purchase` (database
baseline) and the `templateCategories` / `marketplaceTemplates` / `appTemplates` /
`appTemplateVersions` / `templatePurchases` operation families (app-api +
backend-api). It describes behaviour, not decisions of record.

## 0. Answering "does deploy_app_template already exist?"

No. Before this capability the only template-shaped table was
`deploy_build_template`, which is a governed **build recipe** (toolchain +
commands for the build runner), not a marketplace listing. The marketplace is
greenfield: four new tables, two new API operation families per surface.

## 1. Entities

| Entity | Table | Owned by | Purpose |
| --- | --- | --- | --- |
| Template category | `deploy_app_template_category` | platform admin | Browsable taxonomy. One optional parent level (`parent_id`), stable `category_key` slug per tenant. |
| App template | `deploy_app_template` | author user | The listing: display copy, media refs, category, visibility, pricing, moderation state. Points at the source `deploy_app` via `app_uuid` (a reference, not an FK — the template outlives optional app deletion semantics). |
| Template version | `deploy_app_template_version` | author user | An immutable published snapshot: `version`, changelog, packaged artifact reference (`artifact_uuid` → `deploy_artifact`), platform-target snapshot, size/checksum. |
| Template purchase | `deploy_app_template_purchase` | buyer user | The entitlement row, and only that: `ACTIVE` (granted) or `REVOKED` (refund/abuse). FREE templates acquire instantly through the deploy API; a PAID row is written by commerce fulfillment once the order settles, carrying its `order_id`/`order_no`. |

## 2. Lifecycle

Template: `DRAFT → PENDING_REVIEW → PUBLISHED`, with `REJECTED` (back to the
author, with `review_note`) and `DISABLED` (admin kill switch) as side states.
Only `PUBLISHED` + `PUBLIC` templates appear in the marketplace browse surface.

Versions: created `DRAFT`; they become `PUBLISHED` when their template is
approved (the newest approved version becomes `latest_version_uuid`) or, for an
already-`PUBLISHED` template, immediately on create. `WITHDRAWN` removes a
version from acquisition without touching the template.

Purchases: rows are born `ACTIVE` — from the FREE acquire API or from
fulfillment of a settled order — and move to `REVOKED` (refund/abuse). There is
no `PENDING` row and no manual settle action: pre-payment state lives on the
`commerce_order`. An `ACTIVE` row scoped to `(template_id, buyer_user_id)` is
the install entitlement. Acquire is idempotent per `Idempotency-Key`.

## 3. Tenancy and visibility

Everything stays tenant-scoped, exactly like the rest of the module: a template
is visible in the marketplace of **its own tenant only**. Cross-tenant
marketplaces are a future capability that needs an explicit index + policy
change, not an accident of a missing predicate — every browse query filters
`tenant_id = $1` and the repository owner gate follows
`list_domain_zones_repo`.

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

## 4. Payment seam: commerce order + fulfillment port

Money never settles inside the deployments module. A PAID template is bought
through the platform commerce stack:

1. **Order** — the client drives the sdkwork-order checkout
   (`checkout.sessions.*` / module order router, blueprint:
   `sdkwork-routes-order-app-api/src/membership_router.rs`), creating a
   `commerce_order` whose purchase intent carries the template reference.
2. **Payment** — sdkwork-payment owns channels, intents, and webhooks;
   `orders.payments.webhooks.receive` closes the loop.
3. **Fulfillment** — on payment success the order service dispatches by intent
   to a fulfillment port, and the integration adapter (blueprint:
   `sdkwork-order-integration-membership` →
   `PostgresCommerceMembershipStore`) calls this module's exported
   `PostgresCommerceTemplatePurchaseStore::fulfill_paid_template_purchase`,
   writing the ACTIVE entitlement row (with `order_id`/`order_no`/`request_no`
   references) and advancing the install count. Replays are idempotent.

Consequences, all enforced by DDL or code: the deploy acquire API grants
**FREE templates only** (a PAID acquire attempt is a validation error — the
client must go through commerce); the purchase table has **no PENDING state**
(the commerce order owns pre-payment state) and **no manual settle action**
(the backend `templatePurchases.settle` operation was removed); `order_id` is
unique per entitlement (`uk_deploy_app_template_purchase_order`). The deploy
`deploy_app_template_purchase` table therefore remains in the module — it is
the **entitlement/fulfillment projection** (who may install what, install
counts, revocation), exactly the split membership uses — while order state and
payment state are authoritative in sdkwork-order / sdkwork-payment.

## 5. Surfaces

| Surface | Operations | Audience |
| --- | --- | --- |
| app-api `/app/v3/api` | `templateCategories.list`; `marketplaceTemplates.list/retrieve`; `appTemplates.list/create/retrieve/update/delete/submit`; `appTemplateVersions.list/create`; `templatePurchases.list/create` | every client platform (PC, H5, mini-program, Flutter) |
| backend-api `/backend/v3/api` | `templateCategories.list/create/retrieve/update/delete`; `appTemplates.list/retrieve/update/delete`; `appTemplateVersions.list/retrieve`; `templatePurchases.list/retrieve/revoke` | publishing control plane (admin CRUD + moderation + purchase revocation) |

Permissions derive automatically (`deploy.<family>.read|write`); the PC app
manifest lists them in `backend.accessTokenPermissionScope`.

## 6. Client rollout

The capability ships as contract + SDK first. `@sdkwork/deployments-app-sdk`
gains the `marketplaceTemplates`/`appTemplates`/`appTemplateVersions`/
`templatePurchases`/`templateCategories` clients, and
`@sdkwork/deployments-backend-sdk` gains the admin families, so any client
platform consumes the same generated surface:

- **PC** — implemented in this repository: console package
  `sdkwork-deployments-pc-console-marketplace` (browse, detail, acquire, my
  templates, publish) and admin package
  `sdkwork-deployments-pc-admin-app-templates` (categories CRUD, moderation,
  purchase settlement).
- **H5 / Flutter / mini-program** — no such client roots exist in this workspace
  today (`sdkwork-deployments` ships only `apps/sdkwork-deployments-pc`; the
  workspace contains no Flutter app root and `sdkwork-miniapp-engine` has an
  empty `apps/`). They are consumers of the same generated SDK family and need
  their own application roots + `sdkwork.app.config.json` declarations before
  any surface can be authored; that is a client-program decision, not part of
  this capability.
