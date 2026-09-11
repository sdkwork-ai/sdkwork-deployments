# SDKWork Shared Deployment Utilities (infra only)

> **Channel contract (`MODULE_BIN_SPEC.md` §1)**: the operator surface of every
> SDKWork module is its `bin/` nine-entrypoint family. The former cross-module
> deployment orchestrators (`deploy.sh`, `deploy.ps1`,
> `setup-wsl-deployment.sh`, `install-nginx-sites.sh`, `verify-deployment.sh`)
> have been removed — module deployment goes through `bin/docker-deploy.sh`
> with the module's install bundle, and `sdkwork-webserver` owns the public
> reverse proxy edge (no module installs nginx sites).

This directory keeps only the environment/database provisioning utilities that
support compose-based development stacks:

```
scripts/
├── lib/
│   └── common.sh              # Shared port/domain/database/env mapping
├── gateway/                   # Per-module env templates consumed by generate-env.sh
├── generate-env.sh            # Generate per-environment .env files
├── provision-databases.sh     # Provision external PostgreSQL databases
├── bind-windows-hosts.ps1     # Bind Windows hosts file entries
└── README.md                  # This file
```

- **Module-agnostic**: utilities auto-detect the current module or accept
  `--module`. Any SDKWork module can reuse them.
- **Idempotent**: safe to run repeatedly; existing resources are preserved
  unless `--force` is specified.
- **Not an operator channel**: these utilities never start or upgrade an
  application stack. For install/upgrade/rollback/status use the module's
  `bin/docker-deploy.sh` (see the module's `docs/runbooks/deploy.md`).
