# plans

See `DOCUMENTATION_SPEC.md` section 2.

## Active Plans

- [PLAN-2026-0002 Managed Domain And TLS Control Plane](PLAN-2026-0002-managed-domain-tls-control-plane.md) - blocked on the database, API/SDK, security, provider, and runtime review gate declared by its ADR.
- [PLAN-2026-0004 Application Ownership And Super-Admin Ledger](PLAN-2026-0004-app-ownership-and-admin-ledger.md) - proposed; adds an explicit `owner_type` (`PLATFORM`/`TENANT`/`ORGANIZATION`/`USER`) to `deploy_app`, closes the contract/implementation divergence in `AppResponse`, and adds the missing owner gate on `apps.list`.
