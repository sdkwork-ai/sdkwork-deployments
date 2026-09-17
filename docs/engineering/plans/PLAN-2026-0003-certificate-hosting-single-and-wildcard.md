# PLAN-2026-0003 Certificate Hosting: Single-Domain And Wildcard Issuance

Status: active
Owner: SDKWork Deploy maintainers
Updated: 2026-09-15
Requirement: REQ-2026-0003
Decision: ADR-20260723-managed-domain-tls-control-plane
Parent plan: PLAN-2026-0002-managed-domain-tls-control-plane (Phase 3)
Supersedes: none
Specs: ARCHITECTURE_DECISION_SPEC.md, API_SPEC.md, DATABASE_SPEC.md, MIGRATION_SPEC.md,
SECURITY_SPEC.md, CONFIG_SPEC.md, DEPLOYMENT_SPEC.md, OBSERVABILITY_SPEC.md, TEST_SPEC.md

## 1. Objective

Make 托管证书（managed certificate hosting）a commercially deliverable product on the Deploy
control plane by:

1. Splitting certificate intent into two first-class product scopes — **single-domain** and
   **wildcard** — instead of leaving the distinction implicit in per-identifier `EXACT`/`WILDCARD`
   rows that the operator never sees.
2. Completing PLAN-2026-0002 **Phase 3**: the ACME execution engine, DNS-01 challenge orchestration
   (manual operator workflow **and** automatic provider adapters), and certificate budgets.
3. Closing the compliance and abuse-control gaps that block a public CA integration: CAA checking,
   CA quota accounting, duplicate-certificate suppression, and graded expiry alerting.

## 2. Scope And Non-Goals

In scope:

- Certificate scope model, validation, and wildcard SAN planning.
- ACME order/challenge execution engine and its bounded worker.
- Manual DNS-01 (operator pastes a TXT record) and automatic DNS-01 (provider adapters).
- CAA pre-issuance check, CA quota accounting, duplicate suppression.
- App/Backend contract changes and the tenant UI request wizard.

Out of scope (owned elsewhere, unchanged by this plan):

- TLS material delivery, node-scoped snapshots, and activation observations (PLAN-2026-0002 Phase 4).
- KMS/Secret Manager custody selection (Phase 3 item 2, separately gated).
- Custom certificate secret ingest (Phase 3 item 4, separate review per ADR §4).
- `TLS-ALPN-01`; remains disabled per ADR §5.

## 3. Current State Audit

The control plane already has the Phase 1 schema. What is missing is the executor and the product
scope model.

| Layer | Present | Missing |
| --- | --- | --- |
| Schema | `deploy_certificate`, `_identifier` (`EXACT`/`WILDCARD`), `_version`, `_order` (9-state), `_challenge` (`HTTP_01`/`DNS_01`), `deploy_acme_account` | certificate scope column; manual DNS-01 presentation fields; quota accounting |
| Repository | order/challenge/version state machine, optimistic transitions, idempotency | wildcard SAN planning; scope validation; quota enforcement |
| ACME client | — | entire engine (no `instant_acme`/`rcgen` dependency exists) |
| Worker | — | no binary consumes `deploy_certificate_order` |
| Challenge | table only | HTTP-01 presenter; DNS-01 manual + provider adapters |
| Compliance | — | CAA check, CT expectation, quota/rate-limit guards |
| UI | request dialog, list, renew, revoke | scope choice, validation method, challenge instructions, expiry grading |

Evidence:

- `crates/sdkwork-intelligence-deploy-repository-sqlx/src/tls_control.rs:259-262` states the key
  authorization is "produced by the ACME client boundary once credentials exist; the placeholder
  keeps the state machine exercisable end to end" — the boundary is declared, not implemented.
- `deploy_certificate_order` is referenced only from `tls_control.rs`; `grep` over `crates/` finds no
  worker, binary, or job consumer.
- `deploy_certificate` was written as `PENDING` by `create_certificate_repo` with no order,
  and every path that could have advanced it declined: the renewal sweep only selects
  certificates that already carry a validity window, and `renew` only accepted `ACTIVE` or
  `FAILED`. A row that no path will advance is not accepted intent, it is a dead end.
  **Closed 2026-09-16 (phase 3h):** creation requests the first order itself through
  `open_certificate_order`, so a console request inherits the CAA pre-flight, the ACME account
  resolution and the idempotency the operator endpoint and the renewal sweep already share; and
  asking for a certificate that has no version requests that order again rather than being
  refused, which is what makes a first order deferred by a missing ACME account retryable. The
  renewal ledger stays untouched: a first issuance is not a renewal.

Per ADR §2: "A `PENDING` certificate row is an accepted intent, not proof of issuance or
activation." This plan supplies the executor that turns intent into a version.

## 4. Industry Alignment

Reference products: Alibaba Cloud 数字证书管理服务 (CAS), Tencent Cloud SSL, AWS ACM, Cloudflare
SSL/TLS, and Let's Encrypt as the CA policy authority.

| Dimension | Industry behaviour | This plan |
| --- | --- | --- |
| Product scopes | single-domain / multi-domain (SAN) / wildcard, sold and listed separately | `SINGLE_DOMAIN` and `WILDCARD` first-class; multi-SAN remains the union when several verified hostnames are selected |
| Wildcard coverage | one label level only; the apex is **not** covered by the wildcard SAN, products that advertise "泛域名" bundle the apex as a second SAN | selecting `WILDCARD` on apex `example.com` plans exactly `*.example.com` + `example.com` |
| Validation method | HTTP-01 / DNS-01; wildcard requires DNS-01 | `HTTP_01` allowed for exact names only; `DNS_01` required for wildcard and selectable for exact |
| DNS-01 automation | manual TXT for unmanaged providers; API automation for supported providers | manual operator workflow always available; provider adapters additive |
| Rate limits | LE: 50 certificates/registered domain/week, 5 duplicate certificates/week, 300 new orders/3h, 5 failed validations/account/hostname/h | accounting + pre-flight rejection with stable codes |
| CAA | checked before issuance; unauthorized CA blocks issuance | checked before order creation |
| Renewal | automatic, 30 days before expiry, CA-suggested window preferred where available | renewal window honoured; ARI retained when the CA publishes it |
| Expiry alerting | graded 30 / 14 / 7 / 3 / 1 day | same thresholds, surfaced on the certificate list |
| Private key | never exportable for public CA certificates | unchanged; ADR §4 custody rules apply |

## 5. Certificate Scope Model

### 5.1 Values

`CertificateScope` is a property of the certificate aggregate, not of an identifier.

| Scope | Meaning | Identifier plan |
| --- | --- | --- |
| `SINGLE_DOMAIN` | exactly one exact FQDN | `[a.example.com]` |
| `WILDCARD` | one leading-label wildcard and its apex | `[*.example.com, example.com]` |

`WILDCARD` always plans **two** identifiers. `*.example.com` does not cover `example.com`, so a
product that sells "泛域名证书" without the apex silently fails to serve the root host — the exact
defect this model removes.

### 5.2 Validation Rules

1. `WILDCARD` requires the selected hostname to be a wildcard claim (`*.apex`) and the apex claim
   must exist, be `ACTIVE`, and be verified; otherwise the request fails closed.
2. `WILDCARD` requires `validationMethod = DNS_01`. `AUTO` resolves to `DNS_01` for wildcard.
3. `SINGLE_DOMAIN` requires exactly one identifier and it must be `EXACT`.
4. Wildcards deeper than one label (`*.a.example.com` covering `x.a.example.com` only) are
   permitted as exact wildcard claims, but a request may not mix wildcard depths.
5. A bare `*` and multi-level wildcards (`*.*.example.com`, `a.*.example.com`) are rejected at the
   contract boundary, before persistence.
6. Every identifier must be owned by a verified, active claim in the same tenant (ADR §2).
7. `SINGLE_DOMAIN` may use `HTTP_01`, `DNS_01`, or `AUTO`; `AUTO` prefers `HTTP_01` when the edge
   proof path is healthy, otherwise `DNS_01`.

### 5.3 Relationship To Existing Data

`deploy_certificate_identifier.identifier_type` stays `EXACT`/`WILDCARD` (ADR §Data View). The new
`deploy_certificate.certificate_scope` is a derived-but-asserted summary used for validation, UI
grouping, and quota accounting. A database `CHECK` plus the validation rules keep them consistent.

## 6. Contract Changes

`crates/sdkwork-deploy-contract/src/dto.rs`:

- `CertificateScope` and `ValidationMethod` serializable enums with `parse` helpers.
- `CreateCertificateRequest` gains `certificateScope` (default `SINGLE_DOMAIN`) and
  `validationMethod` (default `AUTO`).
- `CertificateResponse` gains `certificateScope` and `validationMethod`.
- `CertificateChallengeResponse` gains the manual DNS-01 presentation surface:
  `dnsRecordName`, `dnsRecordType`, `dnsRecordValue`, `expiresAt` — populated only while the
  challenge is presentable and never after validation.
- New `CertificateQuotaResponse` exposing per-tenant CA budget consumption.
- `StoreCertificateVersionRequest` gains `material` (`CertificateMaterialPayload`): the CA's
  certificate chain verbatim (leaf first), the private key, and an optional root. Required, because
  the endpoint is the only path by which a managed certificate enters the control plane. The four
  digests on the request are re-derived from these bytes and any disagreement refuses the request, so
  the recorded metadata and the stored material cannot drift apart.

The manual DNS-01 record value is a **public** ACME proof (the TXT digest). Only the account key and
private key are secrets; the proof value is returned once, matching the domain-verification
"returned only when an attempt is created" rule in ADR §3.

The chain is one field rather than a leaf plus a separate intermediate list because that is what an
ACME client receives and what `chainSha256` hashes: the issuance worker hashes the blob verbatim, so
asking it to split the blob first would make the digest depend on how it chose to re-join the halves.
`rootPem` is optional because most CAs omit the root; the control plane then resolves the anchor from
the chain itself or from a configured local bundle, and refuses the request if neither applies.

## 7. Schema Changes

Applied to the greenfield baseline (pre-launch; no ordered migrations exist yet) plus
`database/contract/`:

| Change | Detail |
| --- | --- |
| `deploy_certificate.certificate_scope` | `VARCHAR(16) NOT NULL DEFAULT 'SINGLE_DOMAIN'`, `CHECK IN ('SINGLE_DOMAIN','WILDCARD')` |
| `deploy_certificate.validation_method` | `VARCHAR(16) NOT NULL DEFAULT 'AUTO'`, `CHECK IN ('AUTO','HTTP_01','DNS_01')` |
| `deploy_certificate_challenge` | add `presentation_record_name`, `presentation_record_value`, `presentation_expires_at` (bounded; cleared on `VALID`/`CLEANED`) |
| `deploy_certificate_order` | add `caa_checked_at`, `caa_decision` (bounded reason code) |
| `deploy_dns_provider_credential` | new: tenant-scoped provider adapter credential **reference** (`secret://` only), provider kind, status |
| `deploy_certificate_quota_usage` | new: `(tenant_id, window_kind, window_start, scope_key, issued_count, failed_validation_count)` for CA budget accounting |
| `deploy_certificate_material` | new: the version's canonical five-file set (`cert.pem`, `privkey.pem`, `chain.pem`, `root.pem`, `fullchain.pem`), one row per kind |

Constraint discipline follows ADR §Data View: **no plaintext** PEM or key material, no provider
plaintext credentials. Every new column carrying external state has a bounded `CHECK`.

`deploy_certificate_material` is where that discipline is enforced rather than described:

- `chk_deploy_certificate_material_envelope` requires `protection = 'ENVELOPE_AES_256_GCM'` **with** a
  nonce, a wrapped data key, and a `kek_ref` for `PRIVATE_KEY`, and requires `protection = 'NONE'`
  with all three `NULL` for every other kind. A private key cannot be written in the clear and a
  public file cannot be written sealed, whatever the application does.
- `chk_deploy_certificate_material_kind` / `_file` pin the vocabulary and the conventional file names,
  so a row can never carry a name no TLS terminator would load.
- `chk_deploy_certificate_material_digest` / `_size` / `_plain_size` keep `content_sha256` a real
  digest of the **plaintext** and make the recorded size agree with the bytes for uncompressed rows.
- The version row and its five material rows are written in **one transaction**, so a version without
  material — or material without a version — cannot be committed.

## 8. Challenge Orchestration

### 8.1 HTTP-01 (exact names)

Unchanged from PLAN-2026-0002 Phase 3 item 3: publish `/.well-known/acme-challenge/<token>` through
the isolated edge path, narrow precedence, cleaned on success, terminal failure, or expiry.

### 8.2 Manual DNS-01 (always available)

1. Order creation computes per-identifier TXT values and persists them as bounded presentation
   metadata with an expiry.
2. `list_certificate_challenges` returns `dnsRecordName` (the `_acme-challenge.<host>`) and
   `dnsRecordValue` while `status IN ('PENDING','PRESENTING','PRESENTED')`.
3. The operator adds the record at their DNS provider and issues an explicit "check now" command.
4. The worker observes the TXT record from configured vantage resolvers, compares in constant time,
   and only then advances the challenge row to `VALIDATING`; the order advances to
   `CHALLENGE_VALIDATING` once every identifier challenge has validated. (The two names are
   distinct: `VALIDATING` is a `deploy_certificate_challenge.status` value, `CHALLENGE_VALIDATING`
   is a `deploy_certificate_order.order_status` value.)
5. Presentation metadata is cleared on `VALID`, `FAILED`, or `CLEANED`.

A user command may request an immediate check but can never assert success (ADR §3).

### 8.3 Automatic DNS-01 (additive)

`deploy_dns_provider_credential` selects an adapter. Adapters implement create/observe/cleanup for
one provider each; the first tranche targets Alibaba Cloud DNS, DNSPod, and Cloudflare. Adapter
failures downgrade the challenge to manual presentation rather than failing the order, so an
unconfigured or revoked credential never strands a certificate.

## 9. ACME Execution Engine And Worker

New crate `crates/sdkwork-deploy-certificate-worker` plus an ACME engine crate:

- Depends on `instant_acme` and `rcgen` (same libraries the Web Server standalone manager already
  uses, keeping one ACME implementation across the fleet).
- Drives the accepted 9-state order machine:
  `REQUESTED → ACCOUNT_READY → ORDER_PENDING → CHALLENGE_PRESENTING → CHALLENGE_VALIDATING → FINALIZING → VERSION_STORED`
- Claims orders with `FOR UPDATE SKIP LOCKED`, an expiring lease, a monotonic fencing token, bounded
  attempts, and a deadline; a stale worker cannot finalize newer work.
- Validates issued material before storing a version: normalized DNS SAN set equals the request, key
  algorithm matches, the private key matches the leaf SPKI, the leaf is currently valid, and every
  returned hash equals freshly parsed evidence.
- Stores only public evidence in `deploy_certificate_version`; the bundle goes to the secret store
  and the row keeps an opaque `secret://` reference (ADR §4).
- Renewal creates a new order and version; the current valid version stays active until the new
  version reaches activation quorum (PLAN-2026-0002 Phase 4 owns the quorum gate).

## 10. Quota And Rate Limiting

Commercial hosting cannot ship without abuse control: one tenant must not be able to exhaust the
CA's per-domain budget for every other tenant.

Accounting dimensions, recorded in `deploy_certificate_quota_usage`:

| Counter | Window | Default ceiling |
| --- | --- | --- |
| certificates per registered domain | rolling 7 days | 50 |
| duplicate certificate set (same identifier set + key algorithm) | rolling 7 days | 5 |
| new orders | rolling 3 hours | 300 |
| failed validations per (account, hostname) | rolling 1 hour | 5 |
| concurrent orders per tenant | instantaneous | 8 |

Duplicate suppression: a request whose identifier set and key algorithm match an existing
**`ACTIVE`** version within its renewal window returns that version instead of opening a new order.
This both saves budget and matches CA-side duplicate limits.

Rejection is pre-flight: the API returns a stable `409` (budget) or `429` (rate) problem with a
bounded reason code and the retry-after instant. Counters are tenant-scoped and never leak another
tenant's usage.

## 11. CAA And Compliance

- Before order creation, resolve CAA for every identifier (`RFC 8659`). An `issue`/`issuewild`
  property naming a CA other than the configured ACME directory blocks the order with
  `CAA_UNAUTHORIZED_CA`; absence of CAA records permits issuance.
- `WILDCARD` checks the `issuewild` property for the wildcard identifier and `issue` for the apex.
- The decision and observation instant are recorded on the order (`caa_checked_at`, `caa_decision`)
  so a blocked request is auditable without re-querying DNS.
- Certificate Transparency: public CA certificates are logged by the CA; the control plane records
  the issued chain digest so a later CT log comparison is possible. No SCT field is fabricated.

## 12. Renewal And Expiry

- Renewal scheduling continues to use the certificate's renewal window; CA-published ARI windows are
  preferred when available (the standalone manager already consumes `RFC 9773` and the same
  treatment is applied here).
- Graded thresholds `30 / 14 / 7 / 3 / 1` days map to list badges and alert policies.
- A failed renewal never replaces the last known good version (ADR §5).

## 13. Frontend UX

`apps/sdkwork-deployments-pc/packages/sdkwork-deployments-pc-console-delivery`:

1. **Request wizard**: certificate scope selector (单域名 / 泛域名). Choosing 泛域名 states that the
   apex will be included automatically and forces the validation method to DNS-01.
2. **Validation method** selector for single-domain requests (`AUTO` / `HTTP_01` / `DNS_01`).
3. **Challenge panel**: for manual DNS-01 the TXT record name and value with copy controls, an
   explicit "check now" action, and an expiry countdown. Copy never logs the value.
4. **List**: certificate scope column, graded expiry badge, and order progress surfaced from the
   operation record rather than an optimistic UI state.
5. Closing the wizard aborts browser polling only; it never cancels the durable server order.

## 14. Phasing And Exit Evidence

| Phase | Deliverable | Exit evidence |
| --- | --- | --- |
| 3a | Contract + schema (scope, validation method, challenge presentation, quota, DNS credential) | PostgreSQL baseline applies; contract registry updated; DTO parse/round-trip tests |
| 3b | Scope validation and wildcard SAN planning | unit tests: apex auto-inclusion, depth rejection, non-wildcard claim rejection, ownership enforcement |
| 3c | CAA + quota accounting | denial-path tests with stable codes; duplicate suppression reuses the active version |
| 3d | ACME engine + worker | against Pebble: order reaches `VERSION_STORED`; SAN/SPKI/validity mismatch tables reject; lease/fence reclaim |
| 3e | Manual DNS-01 | challenge presentation returned once, cleared after validation; constant-time TXT comparison |
| 3f | Automatic DNS-01 adapters | per-provider create/observe/cleanup tests; credential failure downgrades to manual |
| 3g | Frontend wizard and list | build + real-browser acceptance of the request wizard and challenge panel |
| 3h | First order on creation, and renewal through to a second version | against Pebble: creating a certificate reaches `VERSION_STORED` with no hand-opened order; an expiring certificate is renewed to a second version with a closed ledger; a replayed creation opens no second order; a missing ACME account defers issuance without failing creation |

## 15. Commercial Gate

Managed certificate issuance may be enabled for a tenant only after phases 3a–3g record exit
evidence, the controlled-CA integration passes, no secret appears in SQL, API/SDK models, snapshots,
logs, or support bundles, and quota/CAA denials are exercised. Until then a `PENDING` certificate
remains accepted intent, not issuance — cloud production keeps external TLS termination as the
declared default.
