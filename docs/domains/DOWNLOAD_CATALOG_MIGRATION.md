# Download Catalog and Update Feeds

Deploy owns the download catalog that Cloud Router used to keep as a free-form
JSON document in its site settings. This page records the ownership split, the
schema, the import path for an existing deployment, and the two clients of the
public API.

## Ownership

| Concern | Owner |
| --- | --- |
| Catalog, cards, installers, per-installer update configuration | Deploy (`deploy_download_*`) |
| Package, artifact, release channel, rollout | Deploy release domain (`deploy_package`, `deploy_artifact`, `deploy_release_channel`, `deploy_channel_rollout`) |
| Site branding, homepage copy, footer | Cloud Router site settings (unchanged) |
| The pointer a rendering site stores | Cloud Router `downloads: { "catalogKey": "..." }` |
| Applying the published feed | Desktop client (`sdkwork-birdcoder2` `apps/desktop/src/updates/config`) |

Cloud Router no longer authors installers. Its `/admin/site` downloads tab keeps
only the catalog key plus a deep link to this console; a legacy document that
still contains `cards` stays publishable but is reported as legacy so the
operator can discard it after publishing the Deploy catalog.

## Schema

- `deploy_download_catalog`: one product catalog per
  `(tenant_id, organization_id, catalog_key)`; `status` gates public reads and
  `revision` increments on every publish.
- `deploy_download_card`: the download page groups; `hidden` keeps a card out of
  the public projection without deleting it.
- `deploy_download_item`: one installer per platform/architecture; a direct
  `href` lives in `download_source`, and `package_id`/`artifact_id` link the
  release domain. Product version is `installer_version`; `version` is the row
  version.
- `deploy_download_update`: at most one update configuration per installer
  (`provider`, `feed_url`, `channel_key`, `minimum_supported_version`,
  `check_interval_minutes`, `rollout_percentage`, …).

`deploy_package` gained `checksum_sha512` (required by the updater protocol) and
`feed_metadata`.

## Public endpoints

Anonymous, cacheable (`Cache-Control: public, max-age=60, stale-while-revalidate=300`):

```
GET /app/v3/api/downloads/catalog/{catalogKey}
GET /app/v3/api/downloads/config?product_key=&platform=&architecture=
GET /app/v3/api/downloads/feed/{productKey}/{platform}/{arch}/{channelFile}
GET /app/v3/api/downloads/artifact/{itemUuid}          # 302 to the current location
```

Only `PUBLISHED` catalogs are visible; hidden cards and disabled installers are
omitted; no tenant identifier is returned. A catalog key published by more than
one tenant is refused as ambiguous rather than serving one tenant's catalog to
another's visitors.

The feed is raw YAML (the electron-updater document), not a JSON envelope.
`provider = deployments` generates it from the linked release channel;
`generic`/`github` point at an external feed; `none` pauses updates.

## Importing an existing Cloud Router catalog

The importer lives in `crates/sdkwork-deploy-download-catalog-importer`.

```bash
# 1. Export the stored document (any JSON client):
#    SELECT payload->'downloads' FROM ops_site_runtime_settings WHERE ...;

# 2. Dry run — writes nothing, prints counts, skips and warnings:
node target/debug/download-catalog-importer \
  --input downloads.json --tenant-id 7 --catalog-key cloudrouter-website

# 3. Apply — idempotent upserts by catalog/card/installer key:
SDKWORK_DATABASE_URL=postgres://… node target/debug/download-catalog-importer \
  --input downloads.json --tenant-id 7 --catalog-key cloudrouter-website --apply
```

The report includes `installersWithoutUpdate`: the legacy document carried no
update configuration, so each installer needs a provider and channel in the
console before the catalog is published.

## Cutover

1. Import and inspect the dry-run report.
2. Publish the catalog in the Deploy console (`/admin/downloads/downloadCatalog`)
   and configure each installer's update feed.
3. Set the Cloud Router site settings `downloads` document to
   `{ "catalogKey": "<key>" }` and deploy a build with
   `VITE_DEPLOYMENTS_APP_API_BASE_URL` pointing at the Deploy app API origin
   (CORS must allow the site origin).
4. Desktop builds embed `dshDesktopDistribution { productKey, configUrl,
   allowedHosts }`; a build without it keeps using its packaged `app-update.yml`,
   so both populations stay supported.

## Known follow-ups

- `deploy.downloads.read/write` are not yet in the IAM permission catalog; the
  endpoints currently declare `x-sdkwork-permission: false`.
- Installers that reference a Drive artifact (rather than a direct `href`) cannot
  be downloaded through the public artifact redirect until the deploy drive port
  exposes a presign/download URL resolver.

## Verification status

| Layer | Evidence |
| --- | --- |
| Feed protocol | `cargo test -p sdkwork-deploy-feed` (8 tests: channel names, rollout bucket vector, YAML golden) |
| Public HTTP surface | `cargo test -p sdkwork-routes-deploy-app-api --test downloads_public_routes` (anonymous access, envelope, YAML feed, artifact 302) |
| Persistence | `--ignored` `download_catalog_postgres` tests (publish visibility, idempotent upsert) against real PostgreSQL |
| Full public chain | `--ignored` `downloads_e2e_postgres`: router -> DeployService -> repository -> PostgreSQL (draft invisible, published catalog/config/artifact) |
| Import | `cargo test -p sdkwork-deploy-download-catalog-importer` (6 tests: mapping, skipped targets, disabled without URL, overrides) |
| Desktop update client | `vitest run apps/desktop/tests/updates` (config validation, rollout, cache fallback, schedule interval) |

Not yet covered by an automated test: a feed generated from a real release-domain
row (`deploy_package` + channel), because the fixture needs the app/target/build
FK chain; the generation itself is unit-tested and the route is tested with a
stubbed document.

## Initialization seeds

`database/seeds/seed.manifest.json` ships the BirdCoder catalog for every
environment profile, so a fresh deployment starts with a published catalog
instead of an empty console:

| Profile | Selection | Channel | Version | Interval | Rollout |
| --- | --- | --- | --- | --- | --- |
| `minimal` | schema only | - | - | - | - |
| `standard` / `production` | `SDKWORK_DATABASE_SEED_PROFILE=production` | stable | 0.2.2 | 30 min | 20% |
| `test` | `…=test` | nightly | 0.2.2-rc.1 | 10 min | 100% |
| `development` | `…=development` | nightly | 0.2.2-rc.1 | 5 min | 100% |
| `staging` | `…=staging` | stable | 0.2.2 | 10 min | 100% |

Files: `common/010_birdcoder_catalog_core.sql` (catalog, desktop card, six
installers, six update rows) plus one profile file `011..014`. Rows use reserved
ids (`910001200001` … `910001200306`) and upsert on conflict, so reseeding is
idempotent. The seeds target the platform tenant `100001` (the same convention
as the app store and Cloud Router seeds); change the tenant in the SQL when a
deployment uses a different platform tenant.

Download links follow the packaged artifact names
(`BirdCoder-win-x64.exe`, `BirdCoder-mac-arm64.dmg`, `BirdCoder-linux-x64.AppImage`,
…) under the public dataset base. Update feeds point at
`feeds/<target>/<channel>.yml` and are seeded **disabled** until the channel
document is uploaded; enabling the row in the console is the only manual step.
The packaging side is documented in
`sdkwork-birdcoder2/apps/desktop/docs/RELEASE_MATRIX.md`.
