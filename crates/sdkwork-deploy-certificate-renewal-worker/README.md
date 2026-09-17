# SDKWork Deploy Certificate Renewal Worker

Keeps managed certificates from lapsing. On each planning tick it claims the
certificates that are inside their renewal window with an expiring database
lease, opens one renewal order per claim, and records the attempt. On a slower
tick it retires certificates and versions whose validity window has closed.

Multiple replicas are safe. The lease is what decides which worker owns a
certificate, the ledger's partial unique index makes "at most one open attempt"
a database guarantee rather than a convention, and the order's idempotency key is
derived from the attempt, so a retry of the same attempt reuses the order instead
of spending the tenant's shared CA rate limit twice.

Issuance itself is the existing ACME flow's job — this worker stops at the order.
`renew_before_days` is a certificate-level field (default 30, allowed 7–90) so a
certificate bound to several apps still resolves to exactly one window, and the
window the worker honours is `max(not_before + lifetime/3, not_after −
renew_before_days)`: the floor is what stops a short-lived certificate from being
renewed the instant it is issued.

Configuration (all optional except the worker identity in production-like
environments):

| Variable | Default | Bounds |
| --- | --- | --- |
| `SDKWORK_DEPLOY_CERTIFICATE_RENEWAL_WORKER_ID` | derived from `SDKWORK_NODE_INSTANCE_ID` | ≤128 chars, `[A-Za-z0-9._:-]` |
| `SDKWORK_DEPLOY_CERTIFICATE_RENEWAL_BATCH_SIZE` | 50 | 1–100 |
| `SDKWORK_DEPLOY_CERTIFICATE_RENEWAL_POLL_INTERVAL_MILLIS` | 30000 | 1000–3600000 |
| `SDKWORK_DEPLOY_CERTIFICATE_RENEWAL_LEASE_SECONDS` | 300 | 30–1800 |
| `SDKWORK_DEPLOY_CERTIFICATE_RENEWAL_SWEEP_INTERVAL_MILLIS` | 300000 | 60000–86400000 |
