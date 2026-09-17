# SDKWork Deploy Certificate Worker

Executes certificate issuance (PLAN-2026-0003 §9). On each tick it claims the
`deploy_certificate_order` rows that are ready to make progress, drives each through
the order state machine, asks the ACME engine to issue, validates the returned
material, and stores a version. When an order reaches `VERSION_STORED` the
certificate has a new active version and the renewal ledger row that asked for it is
closed.

It is the second half of the certificate lifecycle. The API accepts intent
(`create_certificate`) and the renewal worker plans renewals; both stop at an order
row, and nothing before this worker turns that row into a certificate. A deployment
that runs the planner but not this worker will accumulate orders that never progress.

It runs as its own process rather than sharing the renewal worker's loop because the
two have nothing in common operationally: planning is a burst of database writes,
while issuance is a multi-second conversation with a third party. Sharing a loop would
let a slow issuance delay the planner and force one timeout to serve both.

## Two rules that decide the design

**The lease is the fence.** Orders are claimed with an expiring lease and every state
transition is conditional on still holding it. A worker that stalls past its lease can
no longer act; it *abandons* the order and does not fail it, because the order now
belongs to another worker and destroying their work would be worse than doing nothing.

**The lease must outlast the CA conversation.** If it did not, a second worker would
re-claim an order whose first worker was still mid-issuance, and the CA would be asked
to issue twice for one intent — spending the tenant's rate-limit budget on a
duplicate. This is checked at startup instead of documented and hoped for.

## Challenge method per domain shape

| Request | Method | What the worker needs |
| --- | --- | --- |
| Single domain, `AUTO` or `HTTP_01` | HTTP-01 | A webroot the public edge serves (`SDKWORK_WEBSERVER_ACME_WEBROOT`) |
| Single domain, `DNS_01` | DNS-01 | A DNS provider credential for the zone |
| Wildcard | DNS-01, always | A DNS provider credential for the zone |

A wildcard name does not resolve to a host, so it has no HTTP proof path: the scope
beats the requested method rather than letting the CA refuse a request the control
plane could already tell was impossible. A wildcard also always carries its apex as a
second identifier, because a wildcard SAN does not cover the bare domain.

## Configuration

All optional except the worker identity in production-like environments.

| Variable | Default | Bounds |
| --- | --- | --- |
| `SDKWORK_DEPLOY_CERTIFICATE_ISSUANCE_WORKER_ID` | derived from `SDKWORK_NODE_INSTANCE_ID` | ≤128 chars, `[A-Za-z0-9._:-]` |
| `SDKWORK_DEPLOY_CERTIFICATE_ISSUANCE_BATCH_SIZE` | 10 | 1–25 |
| `SDKWORK_DEPLOY_CERTIFICATE_ISSUANCE_POLL_INTERVAL_MILLIS` | 15000 | 1000–3600000 |
| `SDKWORK_DEPLOY_CERTIFICATE_ISSUANCE_LEASE_SECONDS` | 900 | 30–1800, and must exceed the ACME operation timeout |

Read by the service host this worker boots, and therefore required for issuance to
happen at all:

| Variable | Purpose |
| --- | --- |
| `SDKWORK_DEPLOY_CERTIFICATE_ISSUANCE` | Set to `0` to disable issuance on this deployment; the worker then exits without claiming anything |
| `SDKWORK_DEPLOY_ACME_PROFILE` | `production` selects the public CA; anything else uses staging |
| `SDKWORK_DEPLOY_ACME_DIRECTORY_URL` | Overrides the directory URL derived from the profile |
| `SDKWORK_DEPLOY_ACME_TRUST_ROOTS_FILE` | PEM roots to trust in addition to the platform store, for an internal or test CA |
| `SDKWORK_DEPLOY_ACME_ACCOUNT_ROOT` / `SDKWORK_DEPLOY_ACME_ACCOUNT_KEY` | Durable ACME account store, so a restart reuses one CA account |
| `SDKWORK_WEBSERVER_ACME_WEBROOT` | Webroot the edge serves HTTP-01 tokens from; the worker and the edge must share it |
| `SDKWORK_DEPLOY_CERT_LIVE_ROOT` | Where the engine keeps certificate files |
| `SDKWORK_DEPLOY_CERTIFICATE_MASTER_KEY_FILE` | Custody key for material at rest; without it no version can be stored |
| `SDKWORK_DEPLOY_TRUST_ANCHOR_FILE` | Local trust anchors, for a chain that stops at an intermediate |
| `SDKWORK_DEPLOY_USE_MEMORY_CLOUD_ACCOUNTS` | Opts DNS credential resolution out of the IAM cloud account center into an in-memory one. The account center is the default because this process already holds a PostgreSQL pool; production refuses the memory value |
| `SDKWORK_DEPLOY_DNS_PROVIDER_KIND` / `_CREDENTIAL` / `SDKWORK_DEPLOY_DNS_ZONE_APEX` | Deployment-wide DNS-01 credential, tried **after** the certificate's account pin, its zone's pin, and the account center. Without any of the four routes a wildcard order fails with "no DNS provider credential" |

## Verification

Unit tests cover the bounded configuration and the lease/operation-timeout invariant:

```
cargo test -p sdkwork-deploy-certificate-worker
```

The end-to-end acceptance — an order reaching `VERSION_STORED` against a controlled
CA, for both HTTP-01 and wildcard DNS-01 — lives in
`crates/sdkwork-intelligence-deploy-repository-sqlx/tests/certificate_issuance_pebble.rs`
and is driven by `deployments/.workbuddy/tmp/pebble/run-e2e.sh`.
