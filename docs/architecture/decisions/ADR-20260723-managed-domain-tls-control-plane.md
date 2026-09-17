# ADR-20260723 Managed Domain And TLS Control Plane

Status: accepted
Requirement: REQ-2026-0001
Owner: SDKWork Deploy maintainers
Date: 2026-07-23
Updated: 2026-07-30
Specs: ARCHITECTURE_DECISION_SPEC.md, API_SPEC.md, INTERNAL_API_SPEC.md, DATABASE_SPEC.md,
MIGRATION_SPEC.md, SECURITY_SPEC.md, PRIVACY_SPEC.md, CONFIG_SPEC.md, DEPLOYMENT_SPEC.md,
NGINX_SPEC.md, OBSERVABILITY_SPEC.md, TEST_SPEC.md

## Context

The accepted cloud publishing architecture assigns domain and certificate control-plane ownership to
SDKWork Deploy and HTTP/TLS execution to SDKWork Web Server. Deploy now has explicit root-domain
Zones, zone-owned hostname resources, expiring DNS TXT attempts, Site bindings, certificate
identifiers, immutable certificate versions, ACME workflow tables, distribution state, TLS runtime
snapshots, listener bindings, and observations. Managed certificate creation is idempotent and may
cover multiple active verified hostnames; the same hostname may be covered by multiple certificate
aggregates. The former site-owned domain API and Drive private-key upload path have been removed.

Production provider execution remains gated: periodic revalidation and takeover holds, ACME/DNS
workers, approved KMS/Secret Manager custody, material distribution, public SNI probes, rollback,
revocation, and operational drills still require implementation evidence. A `PENDING` certificate
row is an accepted intent, not proof of issuance or activation.

Web Server now has a bounded native TLS consumer that can validate an immutable snapshot and mounted
certificate material, build an exact/wildcard SNI index, enforce TLS policy, atomically replace the
Rustls configuration, and recover the last known good snapshot. It deliberately does not own domain
claims, certificate orchestration, or secret custody. A reviewed contract is required to connect the
control plane and data plane without creating a second writable authority or placing private keys in
business storage.

## Decision

### 1. Ownership And Dependency Direction

1. Deploy is the only cloud writer for domain claims, verification attempts, TLS policies, ACME
   accounts, orders, challenges, certificate versions, rollout state, and activation observations.
2. Web Server consumes immutable node-scoped TLS snapshots and authorized mounted material. It
   reports loaded and served evidence but cannot create or mutate a cloud certificate intent.
3. Certificate material is never embedded in a Website runtime descriptor, TLS snapshot, event,
   database column, generated SDK model, log, metric label, or support bundle.
4. Website revisions and certificate versions remain independent. Content changes, Wiki page
   changes, certificate renewal, and certificate rollback do not create each other's revisions.
5. Cross-repository calls use generated SDKs. Deploy publishes TLS assignments through the Web
   Internal SDK; Web nodes report observations through the same Web-owned internal surface.
6. Root-domain zones are owned by a **user subject**, not by the tenant as a whole. The zone
   inventory a console renders is scoped to the calling user: a session sees the zones it owns
   plus the platform-owned tenant-level zones, and nothing else. A zone owned by another user
   answers "not found" to every zone-scoped read and write rather than leaking its hostnames
   or accepting a mutation. Certificate and application resources stay tenant-scoped.

### 2. Domain Identity And Anti-Takeover

Deploy canonicalizes every requested host to lower-case IDNA ASCII without a trailing dot before a
claim is created. Exact and wildcard identities are distinct. Public-suffix apex claims and wildcard
label boundaries are validated before persistence.

A live exact or wildcard identity has one globally exclusive claim. A database transaction and
unique active-claim constraint prevent cross-tenant ownership races. Deletion enters a configurable
hold state before another tenant can claim the identity. A verified claim is periodically
revalidated and is suspended when proof is lost beyond the configured grace period.

Domain lifecycle:

```text
PENDING_PROOF -> VERIFYING -> VERIFIED -> ACTIVE
       |              |          |          |
       +-> FAILED <---+          +-> REVERIFYING
                                  |          |
                                  +-> SUSPENDED -> HOLD -> RELEASED
```

Only `ACTIVE` domains are eligible for public host bindings or managed certificate issuance.
`VERIFIED` proves control but does not by itself activate traffic.

### 3. Domain Verification

Each verification is a durable, expiring attempt. Supported methods are:

- `DNS_TXT`, the default for exact names and the required method for wildcard claims;
- `HTTP_FILE`, allowed only for exact names when the Web edge can serve the isolated proof path;
- `DNS_CNAME`, an optional delegated verification method when the configured provider supports it.

The API returns the proof value only when an attempt is created. Persistence stores its SHA-256
digest, bounded public record/path metadata, expiry, retry schedule, and observation evidence. A
worker performs bounded DNS or HTTP checks. A user command may request an immediate check, but it
cannot assert success. HTTP checks do not follow arbitrary redirects, do not use private or reserved
addresses, and use fixed ports and response-size/time budgets. DNS observations are normalized,
bounded, and compared in constant time where the proof value is secret before publication.

Verification success records the resolver/checker identity, observed value digest, checked time,
and proof method. It never treats possession of the Deploy API credential as domain ownership.

### 4. Certificate Intent, Versions, And Secret Custody

A logical Certificate describes source type, identifiers, renewal policy, and desired/current
version. Certificate Versions are immutable. Each version records only public certificate evidence:
serial digest, leaf SHA-256 fingerprint, SPKI digest, issuer, validity, identifiers, key algorithm,
chain digest, and opaque secret-store references.

**The certificate files themselves are also custody.** The evidence columns describe a certificate;
they cannot reconstruct one, so a version that recorded only evidence could not be delivered to a node
or audited after the CA stopped serving it. `deploy_certificate_material` therefore stores the
canonical five-file set of each version: `cert.pem`, `privkey.pem`, `chain.pem`, `root.pem`, and
`fullchain.pem`.

The rule is **no plaintext key material**, not "no key columns":

- Public material (leaf, intermediates, root, full chain) is stored verbatim. It is public by
  definition, and keeping it readable is what lets an operator answer "which certificate covers this
  name" with a query.
- The private key is sealed by **envelope encryption**. A fresh AES-256-GCM data key (DEK) encrypts
  the file; that data key is wrapped by the custody key-encryption key (KEK) and stored beside the
  ciphertext. The KEK never enters the database, so the row is inert on its own.
- Each sealed file carries AAD binding it to its version id and material kind, so a blob moved
  between rows fails authentication instead of being served under the wrong identity.
- `fullchain.pem` is the leaf followed by its intermediates, byte for byte as the CA returned them,
  and deliberately **excludes the root**. The anchor belongs in the client's trust store; presenting
  it inflates the handshake for no benefit.

Cloud private keys and ACME account keys may additionally live in an approved KMS/Secret Manager. Web
nodes receive short-lived, target-scoped authorization and mount immutable material through an
approved secret delivery mechanism such as Secrets Store CSI. The Web snapshot uses only
`file:<opaque-certificate-version-id>`; the configured material root maps that id to read-only
`fullchain.pem` and `privkey.pem` files.

The custody master key is loaded from a protected secret file, never an environment value or database
row. This holds for standalone deployments using the approved encrypted standalone secret store and
for the managed control plane alike. Standalone self-signed certificates are never eligible for the
cloud production profile.

The trust anchor is resolved from three sources, in order: the root sent with the material, a
self-signed certificate the chain already ends at, or a configured local anchor bundle matched by
issuer name **and** signature. When none applies the request is refused — a stored bundle with an
empty `root.pem` would look like success and validate nowhere.

Custom certificate import uses a one-time secret-ingest session. The private key is streamed over a
protected backend, validated in memory, sealed, and zeroized. It is not uploaded to Drive. The public
chain may be retained in the certificate secret bundle, but Drive node ids are not a certificate or
private-key custody contract.

### 5. Managed ACME Lifecycle

Let's Encrypt production and staging directory profiles are the initial managed CA integration.
Provider ports keep CA, DNS automation, and secret storage replaceable. Exact-name certificates use
HTTP-01 by default when the edge proof path is available; DNS-01 is used for wildcards and may be
selected for exact names. TLS-ALPN-01 remains disabled until the edge challenge listener has its own
bounded activation and conflict tests.

Managed lifecycle:

```text
REQUESTED
  -> ACCOUNT_READY
  -> ORDER_PENDING
  -> CHALLENGE_PRESENTING
  -> CHALLENGE_VALIDATING
  -> FINALIZING
  -> VERSION_STORED
  -> DISTRIBUTING
  -> ACTIVATING
  -> SERVED_VERIFIED
```

Every external operation has an idempotency key, lease/fence, attempt count, next-attempt time,
deadline, bounded provider error code, and terminal/non-terminal classification. Workers use bounded
concurrency and exponential backoff with jitter. Provider responses, challenge values, account keys,
private keys, and secret-store credentials are never logged.

Renewal creates a new order and version. The current valid version remains active until the new
version reaches the configured activation quorum and public served-SNI verification passes. A failed
renewal never replaces the last known good version. Renewal begins before the 30-day threshold and
must complete before the 14-day product SLO threshold.

### 6. Distribution, Activation, And Observation

Deploy compiles a complete, hash-addressed `sdkwork.tls-runtime.v1` snapshot per Web node and
listener. A candidate includes certificate/version identity, authorized material reference,
expected fingerprint, exact/wildcard server names, validity, TLS version range, ALPN, generation,
node identity, and digest. It contains no PEM or secret-provider path.

The Web Internal API adds generated-SDK operations for:

```text
PUT  /internal/v3/api/nodes/{nodeUuid}/tls-runtime-assignments/current
POST /internal/v3/api/nodes/{nodeUuid}/tls-runtime-observations
GET  /internal/v3/api/nodes/{nodeUuid}/tls-runtime-observations/latest
```

The exact OpenAPI operation ids and request/response envelopes must follow `API_SPEC.md` during the
approved implementation. The Edge Runtime validates node scope, generation, digest, material root,
certificate/key match, SAN coverage, validity, fingerprint, listener policy, and ambiguous SNI
ownership before atomic activation. It keeps the last known good configuration on any failure.

An observation distinguishes:

- `RECEIVED`: complete snapshot persisted;
- `MATERIAL_READY`: authorized material resolved and validated;
- `LOADED`: the process atomically selected the version;
- `SERVED`: an authenticated local handshake presented the expected fingerprint for each SNI class;
- `PUBLIC_VERIFIED`: an independent external probe observed the expected fingerprint;
- `FAILED`: bounded stage and reason code without secret or filesystem disclosure.

Deploy advances the certificate's current version only after the policy's node quorum reaches
`SERVED`. Production rollout completion additionally requires `PUBLIC_VERIFIED` from the configured
vantage policy. Observations are monotonic, node-authenticated, generation-fenced, replay-safe, and
retained for audit.

### 7. Rollback, Revocation, And Expiry

Rollback selects a previous non-revoked immutable version and creates a new desired rollout; it does
not mutate history. Revocation marks a version ineligible, creates replacement assignments, and
keeps evidence of CA revocation status and operator reason. Expired, revoked, mismatched, or
unobserved versions cannot become current. Emergency fail-closed policy can remove the SNI mapping
when no safe version exists.

Domain suspension removes public bindings and certificate assignments through independent desired
generations. Reclaiming or deleting a domain never silently transfers an existing certificate or
secret reference to another tenant.

## Data View

The approved implementation replaces the prelaunch simplified domain/certificate baseline. There
is no compatibility table, dual write, backfill, or legacy Drive private-key path.

| Table | Responsibility | Critical constraints |
| --- | --- | --- |
| `deploy_dns_zone` | Explicit root-domain inventory | globally exclusive active normalized apex; user-owned within a tenant, `user_id IS NULL` for platform tenant-level zones; soft delete |
| `deploy_domain` | Canonical hostname claim and lifecycle | globally exclusive active normalized host; tenant/Zone scoped; no Site ownership |
| `deploy_domain_verification` | Expiring proof attempt and observation | token digest only; lease/fence; bounded retry; immutable success evidence |
| `deploy_tls_policy` | Domain/Site TLS source, challenge, renewal, rollout policy | one active policy per binding scope; no secret values |
| `deploy_acme_account` | Tenant/platform CA account metadata | account key secret reference only; directory/profile uniqueness |
| `deploy_certificate` | Logical certificate intent | source type, desired/current version, state, optimistic version |
| `deploy_certificate_identifier` | Exact SAN and wildcard identifiers | normalized unique position; identifier must be owned by active claim |
| `deploy_certificate_order` | Durable ACME order workflow | idempotency, lease/fence, retry, deadline, external reference digest |
| `deploy_certificate_challenge` | Authorization/challenge workflow | proof digest/secret reference only; presentation and cleanup state |
| `deploy_certificate_version` | Immutable issued/imported public evidence | unique fingerprint/serial scope; KMS/secret refs only; no plaintext PEM/key columns |
| `deploy_certificate_material` | The version's canonical file set | one row per kind; private key wrapped by an out-of-database KEK; AAD bound to version and kind; envelope columns required exactly for the secret kind |
| `deploy_certificate_distribution` | Version-to-node material authorization | target-scoped authorization reference, expiry, desired state |
| `deploy_tls_runtime_snapshot` | Complete node/listener generation | unique node/listener/generation; canonical digest; bounded payload metadata |
| `deploy_tls_runtime_assignment` | Snapshot SNI-to-version mapping | unambiguous exact/wildcard owner; expected fingerprint and material id |
| `deploy_tls_target_observation` | Loaded/served/public evidence | node auth, monotonic generation, dedup key, bounded reason code |

Every table follows the standard SDKWork `id`, `uuid`, tenant/ownership, audit, lifecycle, and
optimistic concurrency fields that apply to its profile. High-cardinality workflow tables have
status/next-attempt/lease indexes; tenant reads lead with `tenant_id`; observations are bounded
retention/partition candidates in PostgreSQL.

## API And UI Consequences

The current `domainZones.hostnames.verify` command fails closed and succeeds only after exact DNS TXT
token observation. A newly created attempt returns its proof once; subsequent reads expose only the
digest and bounded attempt metadata. Certificate create accepts `domainIds` and reports accepted
workflow state, not issued/renewed success.

The former custom upload input and `/certificates/upload` operation are removed. A future custom
certificate capability must use a separately reviewed one-time secret-ingest session; it cannot
reuse Drive upload sessions, node identifiers, or ordinary JSON private-key fields. App OpenAPI and
generated SDK families are materialized from this accepted contract.

The console zone inventory is per-user: `domainZones.list` returns only the zones the caller owns
plus the platform tenant-level zones, and every zone-scoped hostname read or mutation re-applies
that same owner gate. Certificates and applications remain tenant-scoped, so a tenant member may
still see a certificate that covers another member's hostname; only the zone inventory itself is
restricted.

Tenant UI must expose domain proof instructions, observed checks, TLS policy, certificate/version
history, renewal, rollout quorum, expiry, rollback, and bounded failure reasons. Admin UI must expose
claim conflicts/holds, CA/DNS/secret-provider health, stuck orders, fleet divergence, served
fingerprints, revocation, and audited recovery actions. No UI can display or download private keys.

## Alternatives

1. Keep metadata-only certificate rows and let operators manage TLS externally. Rejected because it
   cannot satisfy managed renewal, served evidence, rollback, or commercial support.
2. Store custom private keys in Drive. Rejected because Drive is business file storage, not approved
   private-key custody, rotation, or target authorization.
3. Put PEM in the TLS snapshot. Rejected because snapshots are replicated, inspected, and retained
   runtime metadata. This is not in tension with §4: the snapshot still carries no material, only the
   `file:<opaque-certificate-version-id>` reference a node resolves through authorized delivery.
3a. Keep the material only in an external secret store and never in the database. Rejected because the
   version would then be unreconstructable from the control plane's own records: an audit could not
   show what was issued, a re-delivery after a store outage could not proceed, and the ordering
   between "version committed" and "secret written" would be a distributed transaction with a real
   window for a version that has no key. Envelope encryption keeps the rows inert without giving up
   atomicity, and the KEK remains exactly as external as the secret store's own root key was.
3b. Store the private key as a plaintext PEM column and rely on database access control. Rejected
   because it converts every read permission — a backup, a replica, a support query — into key
   disclosure. The table enforces the distinction with a CHECK constraint rather than convention.
4. Let Web Server own ACME and certificate business state in cloud. Rejected because it creates a
   second domain/certificate authority and prevents deterministic cross-site governance.
5. Replace the active certificate immediately after CA issuance. Rejected because issuance does not
   prove distribution, process activation, SNI selection, or public service.
6. Use only an ingress-controller certificate abstraction. Rejected as the platform contract because
   it would not cover native Rustls, standalone, node observations, or provider portability. It may
   implement the distribution port in a specific deployment profile.

## Consequences

- Deploy gains several durable workflow tables, provider ports, workers, App/Backend operations, and
  Web Internal SDK dependencies.
- Web Server gains TLS assignment ingestion/observation contracts and cloud material delivery, while
  its standalone `web_*` certificate manager remains isolated from cloud authority.
- Domain verification and certificate APIs changed before launch; generated SDKs are regenerated
  from the App OpenAPI authority.
- Production native TLS remains disabled until KMS/Secret Manager custody, material distribution,
  node/public observations, rollback, and expiry drills have evidence.
- CA, DNS, KMS/Secret Manager, and external probe providers require explicit production configuration,
  credentials, quotas, alerts, and failure budgets.
- Certificate material custody adds one runtime requirement and one optional one: the custody master
  key file must exist in production-like environments and the service refuses to start without it, and
  a local trust anchor bundle is needed whenever a CA's chain stops at an intermediate and the worker
  did not send the root — which is the normal case for Let's Encrypt.
- Storage now carries key material, so the database's own backup, replication, and access-control
  posture becomes part of the certificate trust boundary. The KEK staying outside is what keeps that
  posture's mistakes from being key disclosure.

## Verification

- PostgreSQL integration tests cover current domain proof, certificate identifier cardinality,
  tenant/status boundaries, idempotency, composition transactions, outbox concurrency, and leases.
- Certificate custody tests cover the full round trip: a real issued bundle is assembled, sealed,
  stored, read back, opened, and compared byte for byte; the private key column is proven not to hold
  the key; a row repointed at another version is proven not to open; a cross-tenant read fails closed;
  a refused storage is proven to leave no material behind. Unit tests pin the digest definitions to
  `sdkwork-webserver-acme-service`, the chain and private-key checks, the anchor precedence, and the
  rejection of a declaration that does not describe the bytes.
- Domain tests cover IDNA, public suffixes, exact/wildcard conflicts, DNS/HTTP proof, rebinding/SSRF,
  expiry, revalidation, hold/reclaim, and cross-tenant races.
- Domain inventory tests prove the zone list is private to its owner: two users in one tenant each
  see only their own zones plus the platform tenant-level zones, and a foreign zone refuses
  retrieve, list-hostnames, update, delete, hostname create, ensure, challenge, and confirm while
  the owner's own reads still succeed.
- ACME tests use a controlled test CA and DNS/HTTP solvers; no production CA is used by CI.
- Secret tests prove no private key/account key appears in SQL, API/SDK models, snapshots, events,
  logs, metrics, traces, crash reports, support bundles, or Drive. The read path that returns opened
  material is deliberately not reachable from any HTTP route.
- Web tests cover material authorization, SAN/key/fingerprint/validity rejection, SNI selection,
  atomic replacement, last-known-good recovery, generation fencing, and served-fingerprint evidence.
- End-to-end staging evidence covers browser DNS -> TLS handshake -> host/path/variant routing ->
  Drive/Wiki content, renewal, renewal failure, rollback, revocation, node loss, and public probes.
- Production enablement requires Security, Database, API/SDK, Operations, and Release approval.

## Production Gate

This ADR authorizes the prelaunch schema/API replacement. Production TLS enablement still requires
human approval of the KMS/Secret Manager, DNS automation, CA, material-delivery and external-probe
profiles, plus security and operations evidence. Provider credentials and live infrastructure
changes remain outside this source change.

## Supersedes / Superseded By

This decision refines the certificate section of
`ADR-20260721-unified-cloud-site-publishing-control-plane.md`. It does not supersede that ownership
decision. It supersedes the simplified site-owned domain model, single-domain certificate metadata,
Drive private-key reference, and planned-only renewal success semantics.
