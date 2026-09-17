# SDKWork Deploy Kubernetes Manifests

Apply order:

1. Create secrets `sdkwork-deploy-database`, `sdkwork-deploy-iam-database`,
   `sdkwork-deploy-web-internal`, `sdkwork-deploy-drive-internal`,
   `sdkwork-deploy-knowledgebase-internal`, and `sdkwork-deploy-certificate` (keys
   `acme-contact-email` and `acme-account-master-key`). Each Internal API secret must contain the
   `ingress-token` key. Install/configure Secrets Store CSI and create the
   `sdkwork-deploy-website-provider-events` and `sdkwork-deploy-certificate-material`
   `SecretProviderClass` resources before starting the workers. The certificate-material class
   projects the envelope-encryption master key as `kek` and, when the chain stops at an
   intermediate, the local anchors as `trust-anchors.pem` — both with file mode no broader than
   `0440`, and both mounted only into the two certificate workers (renewal and issuance), never
   into the gateway.
   Optionally create `sdkwork-deploy-dns-provider` (keys `provider-kind`, `credential`,
   `zone-apex`) to enable automatic wildcard DNS-01 presentation. Without it the issuance worker
   answers "no provider" and a wildcard order fails closed with a bounded reason code instead of
   waiting on a TXT record nobody created.
2. `kubectl apply -f migration-job.yaml` and wait for completion.
3. `kubectl apply -f deployment.yaml`.
4. `kubectl apply -f runtime-assignment-worker.yaml`.
5. `kubectl apply -f certificate-renewal-worker.yaml`.
6. `kubectl apply -f certificate-worker.yaml`.
7. `kubectl apply -f service.yaml`.

Gateway health endpoints:

- `GET /healthz` - gateway process liveness.
- `GET /readyz` - gateway database readiness.

The runtime-assignment, certificate-renewal, and certificate-issuance workers have no HTTP
surface. Kubernetes treats a running process as ready; each exits on invalid production
configuration and uses an expiring database lease so abandoned work can be reclaimed by another
replica.

The certificate renewal worker claims the certificates inside their renewal window and opens one
renewal order per claim; issuance stays with the ACME flow. Like the issuance worker — and no
other workload — it receives the certificate-material `SecretProviderClass`; it needs the
database URL plus the bounded
`SDKWORK_DEPLOY_CERTIFICATE_RENEWAL_*` settings. It shares the Public Suffix-free view of the
database with the gateway, so its renewals become visible to the console as soon as the lease is
taken.

The certificate issuance worker is the other half of that flow: it claims due
`deploy_certificate_order` rows with a leased `FOR UPDATE SKIP LOCKED` claim and drives them to
`VERSION_STORED`. Challenge routing follows the certificate scope — exact names over HTTP-01
against the shared `SDKWORK_WEBSERVER_ACME_WEBROOT`, wildcards over automatic DNS-01 — so the
Public edge must mount the same `sdkwork-deploy-certificate-state` claim and serve
`/.well-known/acme-challenge/` from it, and it needs the bounded
`SDKWORK_DEPLOY_CERTIFICATE_ISSUANCE_*`, `SDKWORK_DEPLOY_ACME_*`, and `SDKWORK_DEPLOY_DNS_*`
settings plus the certificate-material secrets. Its lease must stay strictly greater than the
ACME operation timeout; the worker refuses to start otherwise, because a lease that expires with
an order still in flight would hand live ACME state to a second worker. It runs one replica with
`Recreate` rather than a rolling pair: the ACME account store on the claim is keyed by the stable
worker identity recorded in `deploy_certificate_order.lease_owner`.

The gateway projects the three Internal API credentials into one read-only volume. Drive and
Knowledgebase tokens are used only for provider eligibility checks; the Web token is used for
immutable runtime-assignment publication. The worker projects only the Web and Drive Internal
tokens it needs. Its separate CSI volume exposes per-Web-Node Drive derivation secrets using the
hashed filename contract documented in `etc/README.md`; gateway pods never receive those node
secrets. Secret projection supports rotation without embedding credentials in environment
variables or images. A rotated node secret changes the worker cache fingerprint and causes channel
replacement on the next bounded worker cycle.

The referenced `SecretProviderClass` is platform-owned and intentionally absent from this
application repository because its provider, vault object identifiers, and workload identity are
environment-specific secrets. It must project each assigned Node secret read-only as
`drive-website-node-<lowercase-sha256(nodeUuid UTF-8)>.derivation-secret`, with file mode no broader
than `0440`; startup fails closed when a required file is absent or invalid. The same secret bytes
must be mounted only into the matching Web Node's provider-event configuration. Gateway Pods must
never mount this class.

The worker registers each Drive callback as
`https://web-provider-events.sdkwork.com/nodes/{nodeUuid}/provider-events/drive-website-events`.
The internal HTTPS/mTLS ingress must preserve the complete path and send it to the Node-specific
Web provider-event Service; it must not rewrite to the unqualified Knowledgebase route or
load-balance across a tenant fleet. Deploy renews the Drive channel but is not in the ordinary
content-event data path.

Production pods obtain collision-free Snowflake node ids through the shared database lease
registry. Do not set a static `SDKWORK_DEPLOY_SNOWFLAKE_NODE_ID` in production. The Pod UID is
injected as `SDKWORK_NODE_INSTANCE_ID` to give each worker and gateway process a unique identity.

API surfaces:

- App: `/app/v3/api/*`
- Backend: `/backend/v3/api/*`
