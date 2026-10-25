# SDKWork Deploy
repository-kind: foundation-dependency

SDKWork Deploy is the SaaS-capable Deploy Web Server control plane. It manages web
applications, domains, TLS certificates, Nginx-compatible configuration, build and deploy
pipelines, release history, rollback, health checks, and operational audit.

This repository follows `../sdkwork-specs/SDKWORK_WORKSPACE_SPEC.md`. Application identity is declared in `agent.app.config.json`.

## Standards Alignment

| Framework | Status |
| --- | --- |
| `sdkwork-web-framework` | Integrated — auth, route manifests, v3 `SdkWorkApiResponse` / `ProblemDetail` mapping |
| `sdkwork-database` | Integrated through `database/` assets and `sdkwork-deploy-database-host` |
| `sdkwork-utils-rust` | HTTP envelope types, `parse_bool`, `slugify`, `sha256_hash`, pagination helpers |
| `sdkwork-discovery` | Deferred until RPC services are introduced |
| `sdkwork-drive-app-sdk` | Integrated — Drive-backed upload sessions via `sdkwork-deploy-drive-port` |

## Root Layout

| Directory | Status | Purpose |
| --- | --- | --- |
| `apis/` | active | Authoritative OpenAPI contracts for deploy app/backend surfaces |
| `crates/` | active | Rust service, repository, route, and API server crates |
| `database/` | active | Database contract, baseline DDL, migrations, seeds, drift policy |
| `sdks/` | active | Generated deploy SDK families (`sdkwork-deployments-app-sdk`, `sdkwork-deployments-backend-sdk`); materialized by `pnpm api:materialize` |
| `specs/` | active | Component and topology contracts |
| `etc/` | active | Source-controlled topology profiles, gateway templates, and secret-file references |
| `deployments/` | active | `deploy.yaml`, Docker, and Kubernetes handoff descriptors |
| `scripts/` | active | Dev orchestration and verification entrypoints |
| `docs/` | active | PRD, architecture, standards alignment |
| `tests/` | active | Cross-package contract tests |
| `tools/` | active | OpenAPI and PC package materialization (`materialize_deploy_phase1_contracts.mjs`, `materialize_deployments_pc.mjs`) |
| `apps/` | active | `sdkwork-deployments-pc` Console and backend-admin publishing control plane |
| `jobs/`, `plugins/`, `examples/` | reserved | Future capability directories |

## Development

```powershell
pnpm dev
pnpm check
pnpm verify
```

Database lifecycle:

```powershell
pnpm db:validate
pnpm db:plan
```

## Documentation

- Product Canon: [docs/product/prd/PRD.md](docs/product/prd/PRD.md)
- Technical architecture Canon: [docs/architecture/tech/TECH_ARCHITECTURE.md](docs/architecture/tech/TECH_ARCHITECTURE.md)
- Cloud publishing architecture: [docs/architecture/tech/TECH-cloud-site-publishing-control-plane.md](docs/architecture/tech/TECH-cloud-site-publishing-control-plane.md)
- Unified app delivery architecture: [docs/architecture/tech/TECH-unified-app-delivery-platform.md](docs/architecture/tech/TECH-unified-app-delivery-platform.md)
- Standards alignment: [docs/standards-alignment.md](docs/standards-alignment.md)
- Standards entry: `../sdkwork-specs/README.md`

## Documentation Canon

- [docs/README.md](docs/README.md)
- [docs/product/prd/PRD.md](docs/product/prd/PRD.md)
- [docs/architecture/tech/TECH_ARCHITECTURE.md](docs/architecture/tech/TECH_ARCHITECTURE.md)

## Application Roots

- [apps directory index](apps/README.md)
