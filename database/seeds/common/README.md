# Common seed scripts

Locale-agnostic seed SQL referenced from `seeds/seed.manifest.json` profiles.

- `010_birdcoder_catalog_core.sql` — BirdCoder download catalog shared rows.
- `011..014_birdcoder_catalog_*.sql` — environment profile overrides.
- `015_deploy_app_template_categories.sql` — baseline deployment marketplace
  categories for the platform tenant. It is part of every profile (including
  `minimal`) because the publish API requires a category before a tenant can
  create its first template.
