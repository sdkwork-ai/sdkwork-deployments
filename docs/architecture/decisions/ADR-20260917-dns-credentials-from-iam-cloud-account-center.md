# ADR-20260917 DNS Provider Credentials From The IAM Cloud Account Center

Status: proposed
Requirement: REQ-2026-0003
Owner: SDKWork Deploy maintainers
Date: 2026-09-17
Specs: ARCHITECTURE_DECISION_SPEC.md, APPLICATION_LAYERED_ARCHITECTURE_SPEC.md,
DATABASE_FRAMEWORK_SPEC.md, API_SPEC.md, CONFIG_SPEC.md, SECURITY_SPEC.md, MIGRATION_SPEC.md,
TEST_SPEC.md

## Context

The managed domain and TLS control plane (`ADR-20260723`) needs a DNS provider credential to publish
an `_acme-challenge` TXT record. Every wildcard identifier requires DNS-01, because a wildcard name
does not resolve to a host and therefore has no HTTP proof path at all. Until now the only source was
a deployment-wide credential read from three process environment variables
(`SDKWORK_DEPLOY_DNS_PROVIDER_KIND` / `_CREDENTIAL` / `SDKWORK_DEPLOY_DNS_ZONE_APEX`), plus a dead
`deploy_dns_provider_credential` table the resolver never read.

That design fails the tenancy it runs in:

1. One credential covers the whole process. Two tenants that each own their own zone cannot both have
   automatic wildcard issuance, and a deployment with two zones can only present for one of them.
2. The credential is per-process configuration, so rotating it means redeploying pods rather than
   editing a record.
3. The console has no way to *offer* a choice. A zone and a certificate both need a credential, and
   the operator filling either form has nothing to pick from.

Meanwhile `sdkwork-iam` already owns a unified cloud account center: `iam_provider_account` and
`iam_provider_credential`, three visibility levels (`platform` / `tenant` / `user`), a per-vendor
default, a credential envelope sealed with the module's own key, and a resolution order that walks
narrowest scope first. It was built for object-storage credentials but its vocabulary is generic — an
account carries `vendor_code` and a list of `capability_codes`, one of which is `dns`.

`sdkwork-deployments` and `sdkwork-iam` share one PostgreSQL database and schema. That co-location is
exactly the situation where the wrong move is cheap and tempting: querying `iam_provider_account`
directly from a Deploy repository, or adding a Deploy-owned table that duplicates it. Both would work
today and both would break the moment IAM changes its row shape, and neither would survive the two
modules being deployed apart.

`DATABASE_FRAMEWORK_SPEC.md` settles it: a module whose persistence needs are satisfied by another
module's authoritative data MUST do so through that owner's SDK or RPC facade, not by reading its
tables. `APPLICATION_LAYERED_ARCHITECTURE_SPEC.md` narrows the allowable channels further —
package/crate root exports, generated SDK facades, declared ports, runtime entrypoints. `ADR` review
rules apply, because this decision fixes a data-ownership boundary, a cross-repository dependency
direction, and public naming.

## Decision

### 1. IAM Owns Cloud Accounts; Deploy Consumes Them

IAM remains the only writer of `iam_provider_account` and `iam_provider_credential`. Deploy never
inserts, updates, or deletes a row in either table directly, and adds no table, column, view, or
materialized projection that duplicates them. `deploy_dns_provider_credential` is retired as
`deprecated` in the database table registry with its replacement named; the deployment-wide
environment credential stays, but only as the documented last-resort fallback (§4).

Deploy references an account by an opaque identifier it does not interpret:
`deploy_dns_zone.provider_account_id` and `deploy_certificate.provider_account_id`, both nullable
`VARCHAR(128)`. There is deliberately no foreign key. The two modules share a schema today, but a
foreign key would make Deploy's DDL depend on IAM's table and would encode "these two are always
co-located" into the schema — the same assumption that direct table reads would encode. A dangling id
degrades to "this pin no longer resolves", which falls through to the next step of the resolution
chain instead of failing an order.

### 2. One-Way Dependency Through A Declared Port

`sdkwork-deployments` depends on `sdkwork-iam`; `sdkwork-iam` never depends on
`sdkwork-deployments`, and knows nothing about zones, certificates, or this integration. The
dependency is a path dependency on `sdkwork-iam-provider-account-service`, with no version pin,
matching how `sdkwork-drive` consumes the same module.

The dependency is confined to one crate, `sdkwork-deploy-cloud-account-port`, which contains:

* `lib.rs` — the port trait and its vocabulary (`CloudAccount`, `CloudAccountPage`,
  `ListCloudAccountsCommand`, `RegisterCloudAccountCommand`, `DnsAccountCredential`). Nothing in this
  vocabulary mentions IAM's row shape.
* `iam.rs` — the one file that calls IAM. It translates the account center's row shape, error
  vocabulary, scope resolution, and credential envelope into the port's vocabulary. Every
  IAM-specific detail stops here.
* `memory.rs` — an in-memory adapter for tests and smoke runs.
* `selection.rs` — the environment decision, as a pure function plus a thin `from_env` wrapper.

The service layer and the route layer depend only on the port. Replacing the account center with an
RPC facade — the change co-location currently makes unnecessary — touches `iam.rs` alone.

### 3. Two Binding Points, Certificate Overrides Zone

A zone pins an account for every hostname it owns; a certificate pins one for itself. Both are
optional and both are set through the existing app API, so the console's zone form and certificate
form each gained a picker and neither gained a new resource type.

Resolution order at issuance, first match wins:

1. the account pinned on the certificate,
2. the account pinned on the zone that owns the hostname,
3. the account the account center picks for that zone's declared provider,
4. the deployment-level environment credential.

Steps 1 and 2 are statements an operator made on purpose. Step 3 exists so a tenant that registered
an Aliyun key does not have to also pin it on every zone. Step 4 is how an installation that owns one
zone and never used the account center keeps working unchanged.

A resolution that falls through to step 3 **does not record the account it chose**. Writing the
derived id back would freeze a decision made against that day's inventory onto a certificate that
outlives it: an account that is later re-registered, disabled, or replaced would leave the
certificate pointing at an id that no longer exists, and the failure would surface at renewal time
rather than at the change.

### 4. The Deployment-Level Credential Becomes The Fallback

`SDKWORK_DEPLOY_DNS_PROVIDER_KIND` / `_CREDENTIAL` / `SDKWORK_DEPLOY_DNS_ZONE_APEX` keep working
exactly as before and are tried last. This is deliberate rather than transitional politeness: a
single-zone installation with no account center is a real and supported shape, and it is the shape
every existing deployment is in. The variables gain no new semantics; the only change is that three
other routes now get a chance to answer first.

The fallback is wrapped rather than replaced. `AccountBackedDns01PresenterResolver` tries the account
center and delegates to whatever the host configured when it declines, so a deployment with neither
an account nor the environment variables still answers "no provider" rather than erroring — which is
what lets the worker fail an order with a bounded code instead of waiting on a TXT record nobody
created.

### 5. Reuse Before Create, And The Console's Pre-Check

Registering an account looks for an existing one for the same vendor and scope first, and reuses it
rather than minting a second. This is the account center's own rule applied consistently: two
accounts for one vendor with neither marked default already resolve to `Conflict`, so silently
creating the second is never what the caller meant.

The listing endpoint is therefore also the pre-check. `GET /app/v3/api/cloud_accounts` filtered by
`dnsProvider` answers "does an account for this family already exist", and a non-empty page means the
form can skip the credential fields in favour of pinning what is there. `CloudAccountPage.items` is
**narrowest scope first, each level's default ahead of its other candidates** — the same precedence
the server resolves in, so `items[0]` is the account a create would reuse whenever the choice is
unambiguous. That order is contract, not an implementation detail: without it stated, every console
would re-derive the rule and some would get it wrong.

Registering a reused account answers `201` with `reused: true` rather than a distinct status. The
caller's next step is identical either way, and the endpoint is marked idempotent; what the response
must not do is claim a row was inserted when one was merely found, which is why `reused` and
`credentialApplied` are in the body.

A reused account that **already has** a credential is left alone. Rotating a secret another business
module may be using is not this flow's decision to make; the caller who wants that is asking for a
rotation, which this endpoint does not offer.

### 6. Scope Vocabulary And What Deploy Does Not Expose

Deploy maps its picker onto IAM's existing scopes and adds none:

* `platform` and `tenant` accounts are shown as `tenantGlobal: true` — any member of the tenant may
  use them. The flag is derived from `scope_type` rather than sent by IAM, so the two surfaces cannot
  drift into disagreeing about what "global" means.
* `user` accounts are shown as `tenantGlobal: false` and are the caller's own.

Deploy does **not** expose IAM's `organization` scope. Deploy's own request context carries an
organization id, but its cloud account contract has no organization field, and enabling a visibility
layer while saying nothing about which organization would be asking the account center to guess. The
listing therefore passes `organization_id: None` and `include_organization_shared: false`. When a
Deploy surface needs organization-scoped accounts, the field is added to the contract in the same
change that turns the layer on.

Registering a `platform`-scope account from the app API is **not** granted. The account center
restricts platform accounts to callers inside the platform tenant, and an app-API session is an
ordinary tenant member's. A request that names `platform` reaches the account center and is refused
there, which is an honest failure with an accurate message; the alternative — letting Deploy decide —
would put a security rule in the module that does not own it.

### 7. The Environment Dimension Is Deliberately Not Translated

Deploy's environments are `dev` / `test` / `staging` / `demo` / `production`. The account center's are
`development` / `sandbox` / `production`. They are not the same vocabulary and there is no correct
mapping between them: `demo` is not `sandbox`, and `staging` has no counterpart at all. Deploy
therefore passes `environment: None` through the port and does not offer an environment filter in the
picker. Any translation here would be invented, and an invented mapping is worse than a missing
feature because it silently selects the wrong credential.

If the two vocabularies are ever unified, the filter is added at the port boundary in one place.

### 8. An Unconfigured Account Center Is Not An Empty One

`SDKWORK_DEPLOY_USE_MEMORY_CLOUD_ACCOUNTS` inverts the convention the other Deploy provider ports
follow. For Drive and the content provider, memory is what local development uses and the flag
switches to the real thing. Here the real thing needs only the PostgreSQL pool the process already
holds, so the account center is the **default in every profile** and the flag is the opt-out.

The selection function enforces two consequences:

* Production **refuses** the memory value rather than honouring it, `Err` rather than a quiet
  downgrade. A `SDKWORK_DEPLOY_USE_MEMORY_CLOUD_ACCOUNTS=1` left over from a development env file must
  not be able to point a production process at a fake account center.
* With no pool and no memory opt-in the port is `Unconfigured`, and every operation on it reports
  "the account center is not configured" rather than an empty result set. "This tenant has no DNS
  account" and "nobody wired the account center" look identical to an operator and mean opposite
  things; the first is a tenant fact they can act on and the second is a deployment fault.

Zones and certificates may still be created with no account bound, and their DNS-01 then falls
through to the deployment-level credential. That is a supported state, not a partially configured
one.

## Consequences

* Two modules now share a schema *and* a compile-time dependency, in one direction. The schema
  sharing is pre-existing (production injects one `SDKWORK_DATABASE_*` identity); the dependency is
  new and is the reason the direct-read alternative was rejected.
* A Deploy build now compiles `sdkwork-iam-provider-account-service`. A break in that crate blocks
  Deploy, so Deploy inherits IAM's release cadence for this one crate. The port confines the blast
  radius to `iam.rs` for source changes, but not for compilation.
* `provider_account_id` is a plain string with no referential integrity. An id that no longer resolves
  is indistinguishable from an id that never existed, and both fall through to the next step of the
  chain. Diagnosing "the pin stopped working" means reading the resolution log line, which is why
  that line reports the hostname, the zone apex, the provider, and the account id together.
* Accounts deleted or disabled in IAM while a zone still pins them are not detected until an issuance
  runs. A pre-flight check at zone/certificate write time would catch it earlier at the cost of
  failing a write on another module's transient unavailability; the current choice prefers the write to
  succeed and the failure to land at the operation that actually needs the credential.
* The certificate worker and the renewal worker both resolve through the same chain, so a wildcard
  renewal uses whatever the account center resolves for the same hostname. Nothing about the renewal
  path is special-cased, and the renewal worker needs no DNS secret of its own.
* Adding `vendor_code = cloudflare` to IAM's documented vendor list was required for a Cloudflare
  account to be creatable from the console. The column accepts any lowercased token; the list is what
  the console offers.

## Compliance

* `DATABASE_FRAMEWORK_SPEC.md` — cross-module consumption goes through the owner's facade; no
  duplicated authoritative table; `deploy_dns_provider_credential` marked `deprecated` with its
  replacement named in the table registry.
* `APPLICATION_LAYERED_ARCHITECTURE_SPEC.md` — the channel is a declared port
  (`deployCloudAccountPort`) with the provider named in `component.spec.json`, plus a runtime
  entrypoint (`cloud_account_port_from_env`).
* `API_SPEC.md` / `API_ASSEMBLY_SPEC.md` — the two new app-API routes are published through the
  existing `sdkwork-deployments` contribution; no second contribution was created for the same owner,
  and the route manifest subset that the standalone gateway mounts was widened to include them.
* `CONFIG_SPEC.md` — `SDKWORK_DEPLOY_USE_MEMORY_CLOUD_ACCOUNTS` is declared in the port, host,
  assembly, worker, and repository component specs, in the topology env files, and in the Kubernetes
  manifests.
* `SECURITY_SPEC.md` — the secret half never crosses the port's `Debug`, is never rendered into an
  error, and is translated into each provider's own field names inside Deploy rather than stored in
  the account center's generic shape.
* `TEST_SPEC.md` — the selection matrix, the vendor↔family mapping, the reuse-before-create rule, and
  the "unconfigured reports unavailable" property are unit-tested; the resolution chain is exercised
  end-to-end against a controlled CA.
