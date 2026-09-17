# Architecture Decision Records

New ADRs use `ADR-YYYYMMDD-<short-title>.md` in this directory.

Retired layout: `docs/adr/` must not be used for new ADRs.

See `ARCHITECTURE_DECISION_SPEC.md`.

## Active Decisions

- [ADR-20260721 Unified Cloud Site Publishing Control Plane](ADR-20260721-unified-cloud-site-publishing-control-plane.md) - accepted cross-repository ownership and live resource model.
- [ADR-20260723 Managed Domain And TLS Control Plane](ADR-20260723-managed-domain-tls-control-plane.md) - proposed durable domain proof, certificate custody, ACME, distribution, and served-evidence model.
- [ADR-20260804 Unified App Delivery Platform](ADR-20260804-unified-app-delivery-platform.md) - accepted tenant App aggregate, source/build/package/version/channel/deployment model for web, API, mini-programs, mobile, and HarmonyOS applications.
- [ADR-20260917 DNS Provider Credentials From The IAM Cloud Account Center](ADR-20260917-dns-credentials-from-iam-cloud-account-center.md) - proposed consumption boundary for the account center, the zone/certificate pin chain, and the retirement of the deployment-local credential table.
