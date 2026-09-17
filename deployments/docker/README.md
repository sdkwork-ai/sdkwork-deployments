# SDKWork Deploy Docker Image

The image contains four process entrypoints:

- `sdkwork-api-deployments-standalone-gateway` serves the public application APIs.
- `sdkwork-deploy-runtime-assignment-worker` claims durable runtime assignments and publishes them
  to Web Server through the generated Web Internal SDK.
- `sdkwork-deploy-certificate-renewal-worker` renews managed certificates before they expire and
  retires the ones whose validity window has closed. It plans; it does not issue.
- `sdkwork-deploy-certificate-worker` claims the due certificate orders that planner opened and
  drives each one to a stored version: HTTP-01 for exact names, automatic DNS-01 for wildcards,
  with the issued material validated and sealed before it is stored.

Run one process per container. The default command starts the gateway; override the command with
any worker name for worker replicas. All process types use the same immutable image digest and
database schema.

Build from the SDKWork workspace root so Cargo sibling dependencies are available:

```bash
docker build -f sdkwork-deployments/deployments/docker/Dockerfile -t sdkwork-api-deployments-standalone-gateway:local .
```

Run against the workspace PostgreSQL development database:

```bash
docker run --rm -p 3900:3900 \
  -e SDKWORK_DATABASE_URL=postgresql://sdkwork_ai_dev:change-me@host.docker.internal:5432/sdkwork_ai_dev \
  -e SDKWORK_DATABASE_SCHEMA=sdkwork_ai_dev \
  -e SDKWORK_DATABASE_AUTO_MIGRATE=true \
  sdkwork-api-deployments-standalone-gateway:local
```

> The container listens on **3900** (spec: `ENVIRONMENT_SPEC` server-bind / `NGINX_SPEC`;
> the legacy `8080` binding is obsolete). Map the host port to **3900**, not 8080.

### Compose deployment (recommended)

Use the bundled Compose files in this directory for the external-dependency mode
(PostgreSQL/Redis run outside the stack):

```bash
# From a module workspace that produced the env files at ./docker/env/<env>.env
docker compose \
  -f sdkwork-deployments/deployments/docker/docker-compose.yml \
  -f sdkwork-deployments/deployments/docker/docker-compose.external.yml \
  --env-file ./docker/env/development.env \
  -p sdkwork-api-cloud-gateway-development up -d
```

The `deploy.sh` / `deploy.ps1` scripts wrap this exactly and fall back to the
bundled templates automatically. The generated env sets
`SDKWORK_DEPLOY_DEPLOYMENT_PROFILE=standalone` so the container runs in
standalone (external-dependency) mode, matching `PROFILE_ID=standalone.<env>`.

Production deployments must inject the shared PostgreSQL identity through `SDKWORK_DATABASE_*`.
Deploy and IAM modules use that same database and schema; no module-specific database secret is
supported. Production deployments must
also set `SDKWORK_DEPLOY_USE_MEMORY_DRIVE=false`, `SDKWORK_DEPLOY_USE_MEMORY_CONTENT_PROVIDER=false`,
`SDKWORK_DEPLOY_USE_MEMORY_CLOUD_ACCOUNTS=false`, `SDKWORK_DRIVE_FACADE_URL`, the Drive/Knowledgebase
Internal API URLs, and
`SDKWORK_DEPLOY_WEB_INTERNAL_API_URL`. Mount each Internal API ingress token as a read-only file at
the path declared by its corresponding `*_INGRESS_TOKEN_FILE` key. Production Snowflake ids use
the shared database lease allocator; static node ids are rejected.

`SDKWORK_DEPLOY_USE_MEMORY_CLOUD_ACCOUNTS` is the one provider flag whose memory value is *not* how
a development stack reaches the real thing. DNS provider credentials live in the IAM cloud account
center, which Deploy reaches over the same PostgreSQL connection the `SDKWORK_DATABASE_*` identity
already opens, so the account center is the default in every profile and this flag exists only to
opt out. Production refuses the memory value outright rather than honouring it.

The certificate renewal worker does not serve HTTP and needs only the database URL, the bounded
`SDKWORK_DEPLOY_CERTIFICATE_RENEWAL_*` settings, a unique `SDKWORK_NODE_INSTANCE_ID`, and the
certificate-material master key referenced by `SDKWORK_DEPLOY_CERTIFICATE_MASTER_KEY_FILE`. Like
the certificate issuance worker — and no other process — it receives that key file; without it the
worker still claims and orders renewals, but storing the issued version fails closed rather than
writing a private key in the clear.

The certificate issuance worker does not serve HTTP and needs the database URL, a unique
`SDKWORK_DEPLOY_CERTIFICATE_ISSUANCE_WORKER_ID`, the bounded
`SDKWORK_DEPLOY_CERTIFICATE_ISSUANCE_*` settings, the ACME settings
(`SDKWORK_DEPLOY_ACME_*, SDKWORK_DEPLOY_CERT_RENEW_BEFORE_DAYS`, `SDKWORK_DEPLOY_CERT_LIVE_ROOT`),
the certificate-material master key file, and a writable `SDKWORK_WEBSERVER_ACME_WEBROOT` that the
edge serving `/.well-known/acme-challenge/` also mounts. Its issuance lease must stay strictly
greater than `SDKWORK_DEPLOY_ACME_OPERATION_TIMEOUT_MS`; the process refuses to start otherwise.
Automatic wildcard DNS-01 additionally resolves a credential in this order: the account pinned on the
certificate, the account pinned on the zone that owns the hostname, the account the IAM cloud account
center picks for that zone's declared provider, and finally the deployment-wide
`SDKWORK_DEPLOY_DNS_PROVIDER_KIND`, `SDKWORK_DEPLOY_DNS_PROVIDER_CREDENTIAL`, and
`SDKWORK_DEPLOY_DNS_ZONE_APEX` below. The three variables are the last resort rather than the only
route; without any of the four a wildcard order fails closed with `ORDER_ERROR_VALIDATION_FAILED`
instead of waiting on a TXT record that was never created.

The runtime-assignment worker does not serve HTTP and does not need the Drive App SDK or
IAM/CORS/listener configuration. It requires the workspace database URL, Web and Drive Internal URLs and ingress-token
files, the HTTPS provider-event callback base, the protected per-Web-Node derivation-secret
directory, a unique `SDKWORK_NODE_INSTANCE_ID`, and the bounded runtime-assignment batch, polling,
lease, expiration, and renew-before settings from the selected topology profile. Kubernetes
injects the Pod UID as the process identity.

The callback base is an HTTPS origin or path prefix. The worker appends
`/nodes/{nodeUuid}/provider-events/drive-website-events`, registers the result through the generated
Drive Internal SDK, and renews it before expiry. The ingress must route the resulting path to the
matching Web Node without a fleet-wide load balancer or path rewrite. Deploy is the channel
registration/renewal controller; it does not receive or acknowledge ordinary Drive content events.
