# DNS Provider Coverage

> Status: **the provider set is open, not enumerated.** Four families are implemented, and the
> fourth one — `HTTP_REQUEST` — carries a provider's requests as *data*, so a vendor with an
> HTTPS API is configured instead of coded. This document is the audit of what the platform
> can drive, every surface that has to agree with it, and what remains genuinely unsupported.

## 1. What is implemented today

Four provider families have a real adapter, a credential shape, a vendor mapping, a console
form, and — for the three named vendors — a read-only **account probe**:

| Family | Vendor code | Credential shape | Adapter | Account probe |
|---|---|---|---|---|
| `ALIYUN_DNS` | `aliyun` / `ali` | `access_key_pair` (AccessKeyId + AccessKeySecret) | `dns_aliyun.rs` | `DescribeDomainRecords`, `PageSize=1` |
| `DNSPOD` | `tencent` / `dnspod` / `qcloud` | `access_key_pair` (LoginId + ApiToken) | `dns_dnspod.rs` | `Domain.Info` |
| `CLOUDFLARE` | `cloudflare` / `cf` | `bearer_token` (ApiToken) | `dns_cloudflare.rs` | `GET /zones?name=<apex>` |
| `HTTP_REQUEST` | `custom` | `secret_text` (a request-configuration document) | `dns_http_request.rs` | none — see below |

`manual` is a fifth *declaration*, not a family: it means the records are published by hand, so
no account is involved and the operator-facing presenter is used.

`HTTP_REQUEST` is the one that changes the shape of the question. The other three answer "which
providers are supported" with a list that is settled per release; `HTTP_REQUEST` answers it with
"any provider whose API is reachable over HTTPS", by taking the endpoint, the headers, the body
template and the secrets from the account's own configuration. A provider added this way needs no
Rust, no contract change and no deploy.

It is also the one family that cannot be probed: its endpoints are the operator's own templates,
so there is no zone-less call that proves the account, and inventing one would mean picking an
endpoint the operator never configured. `verify_account` therefore answers
`DnsAccountVerification::Unsupported` for it — reported as "not checkable", never as "checked",
and never as a reason to refuse an order.

### 1.1 What a probe refusal means

`Dns01Presenter::verify_account` is one read-only call that answers "can this account present for
this zone?". Its *error variant* is part of the contract, because the caller probes before it
writes and has to tell two things apart:

| Family | Refusals that prove the account is wrong (`Config`) | Refusals that prove nothing (`Provider`) |
|---|---|---|
| `CLOUDFLARE` | `401` / `403`, code `10000`, a `200` whose zone list omits the apex | rate limits, `5xx`, unknown codes |
| `ALIYUN_DNS` | `InvalidAccessKeyId.NotFound`, `SignatureDoesNotMatch`, `IncorrectDomainUser`, `InvalidDomainName.NotFound` | `QuotaExceeded.Record` (the 90-per-name ceiling), `Forbidden.RAM`, `RecordForbidden.*` |
| `DNSPOD` | `-1` ("登陆失败") only | `-2` (**"API used too frequently" — a rate limit, not a token error**), `-7`, `-8`, `-99` |
| `HTTP_REQUEST` | — (not probeable) | — |

The default for an unrecognised refusal is `Provider`. That direction is the whole point: the probe
is an **extra** call, the order does not need it to succeed, so a code nobody has classified can
never block an order. Getting it backwards would turn a provider's bad minute into a lost
certificate, which is worse than not probing at all.

`-2` is called out because it is the trap: it reads like a token problem and is DNSPod's rate
limit. Treating it as a credential verdict would fail a working account the first time an operator
retried quickly.

## 2. Every surface that repeats the family list

One family is spelled in four places. Each one fails differently when it lags, and each
failure lands on an operator rather than on a build:

| # | Surface | Where | If it lags |
|---|---|---|---|
| 1 | `dns_provider::ALL` (+ `normalize` / `is_supported` / `vendor_code_for` / `family_for_vendor_code` / `credential_kind_for`) | `crates/sdkwork-deploy-cloud-account-port/src/lib.rs` | a family the engine can drive cannot be registered or offered |
| 2 | `DnsProviderKind` (+ `ALL` / `DEDICATED` / `as_str` / `parse`) and one adapter per variant | `sdkwork-webserver/crates/sdkwork-webserver-acme-service/src/dns*.rs` | stored credential is refused at the first order: "unsupported DNS provider kind" |
| 3 | closed `dnsProvider` enums | `apis/app-api/deploy/openapi.yaml` → materialised JSON → generated SDK | the console offers a family the server cannot drive, or cannot offer one it can |
| 4 | console vocabulary (`DNS_FAMILIES`, `dnsFamilyFromDeclared`, `dnsFamilyLabel`, `CLOUD_ACCOUNT_CREDENTIAL_FIELDS`, i18n) | `apps/sdkwork-deployments-pc/packages/sdkwork-deployments-pc-console-delivery/src/` | a family renders as its raw token, or its form asks for the wrong fields |
| 5 | the size bound on a request configuration | `secretAccessKey.maxLength` (contract) and `MAX_HTTP_REQUEST_CONFIG_BYTES` (engine) | a document the console accepts is one the presenter refuses — a registered account that can never present a challenge |

`CLOUD_ACCOUNT_CREDENTIAL_FIELDS` is a `Record<CloudAccountDnsProvider, …>` and the union
is generated from surface 3, so **the credential *forms* in surfaces 3 and 4 cannot drift**:
adding a family to the contract turns the console into a compile error until its credential
form and labels exist.

That compile-time guarantee does **not** extend to `DNS_FAMILIES`. The array is typed
`readonly CloudAccountDnsProvider[]`, which forces every entry to be a valid member but
says nothing about *membership*: dropping a family from it compiles cleanly, and the result
is a provider the server accepts, the engine drives, and no operator can select. It is
closed by the reachability gate in §2.2 rather than by the type system.

### 2.1 The gate that makes them move together

`crates/sdkwork-intelligence-deploy-repository-sqlx/tests/dns_provider_parity.rs` reads
surfaces 1–3 and the bound in surface 5, and fails on **either** side of each comparison:

* the ACME engine names exactly `dns_provider::ALL`, and every family round-trips through
  `DnsProviderKind::parse` / `as_str`;
* the contract's closed enums equal `dns_provider::ALL`, and the *set of sites* that carry
  a provider is discovered and compared with a hand-maintained list, so a new closed list
  cannot be added silently;
* the three zone-declaration sites are asserted to stay **open** text, because `manual` and
  any not-yet-integrated provider must remain expressible;
* every family declares a vendor, a credential shape and a vendor round-trip, and can be
  built into a presenter credential — the hop that otherwise fails at issuance rather than
  at registration;
* the two bounds agree numerically: `CreateCloudAccountRequest.secretAccessKey.maxLength`
  is `MAX_HTTP_REQUEST_CONFIG_BYTES`, so a configuration the console accepts cannot be one
  the presenter refuses;
* `HTTP_REQUEST` is pinned by name (a set-equality test would keep passing if it were
  quietly dropped, and dropping it is the change that closes the set again), and the
  engine's `DEDICATED` list is pinned to the frozen DDL list;
* an unknown family is refused by both halves.

### 2.2 The gate for the surfaces that are not lists

`crates/sdkwork-intelligence-deploy-repository-sqlx/tests/dns_provider_reachability.rs`
closes the other half of §2: a family can be spelled correctly in all five surfaces and
still be **unreachable**, which no list comparison can see.

| # | Property | Why it is not covered by the type system |
|---|---|---|
| 6 | every family's vocabulary entry becomes a credential **and** a presenter that reports that same family | the credential layer and the adapter are separate match arms; a family can be added to one and not the other |
| 7 | the console offers **exactly** the families the engine can drive (set equality, both directions) | `DNS_FAMILIES` is a hand-written array; §2 explains why its type does not force completeness |
| 8 | the console asks for an identifier on **exactly** the families the server requires one from | `Record<…>` forces the families to be present, not the *fields* to match the server |
| 9 | the issuance path **calls** the provider probe, and calls it before the order is presented | "this function is never invoked" is invisible to any signature — see below |
| 10 | a probe refusal is reported as the variant that carries a detail onto the order | the classification and the order's detail extractor live in different functions |

Property 9 is the one that was actually violated. All three adapters implemented and
unit-tested `verify_account`, and **no host called it**: a wrong DNS credential was still
discovered by the CA several round trips into an order. A capability that exists, is
tested, and is unreachable is the failure mode this half of the gate exists for, and it is
why the assertion is over the call and its position rather than over a type.

## 3. The DDL is deliberately not part of that

The platform's only `CHECK (provider_kind IN (…))` sits on
`deploy_dns_provider_credential`, which the baseline itself declares **DEPRECATED** ("no
code path reads or writes it"). A live family is *not stored* anywhere: accounts live in
the IAM account center under a free-form `vendor_code`, and the family is derived from it
by `family_for_vendor_code`.

So adding a family needs **no migration** — and the gate exempts that constraint while
asserting the deprecation note is still there. The day the note is removed, that frozen
list becomes the live one and the gate fails until it is made equal to
`dns_provider::ALL`.

## 4. What "all industry providers" means, and where we stand

The de-facto industry list is what the leading ACME clients ship. **lego** (the library
behind Traefik, Caddy and many others) supports roughly **180** providers; `acme.sh` a
similar number; the Let's Encrypt community keeps a curated list of providers whose API is
open to all users and whose updates land within 30 minutes.

Today we are at **3 of ~180 *native* adapters** — but the fourth family is not a vendor, it is a
mechanism: `HTTP_REQUEST` drives any provider with an HTTPS API from the account's own request
templates, so the practical answer to "do you support provider X" is already "yes, if its API is
reachable over HTTPS", and the count below tracks only what ships as Rust. The gap is not evenly
weighted: a handful of families cover most deployments, and two further *mechanisms* in §4.2 cover
a long tail at once.

### 4.1 Tiers

**Tier 1 — on every industry list (12 families)**

| Family | Notes |
|---|---|
| Amazon Route 53 | SigV4; the single most widely used |
| Google Cloud DNS | service-account JWT (RS256) |
| Azure DNS | OAuth2 client credentials |
| Cloudflare | ✅ done |
| Alibaba Cloud DNS (阿里云) | ✅ done |
| Tencent Cloud DNS / DNSPod | ✅ done |
| 华为云 DNS | AK/SK signature |
| 火山引擎 DNS | AK/SK signature |
| 百度智能云 DNS | AK/SK signature (`bce-auth-v1`) |
| GoDaddy | `sso-key` header; large registrar share |
| Namecheap | API key + IP allowlist; large registrar share |
| DigitalOcean | bearer token; trivial |

**Tier 2 — one adapter covers a whole segment**

| Mechanism | What it covers |
|---|---|
| **RFC 2136** (`dnsupdate`) | BIND, PowerDNS (authoritative), Knot, Windows DNS, Infoblox, and any standards-compliant server — most of self-hosted and enterprise |
| **PowerDNS HTTP API** | PowerDNS with the API enabled, including hosted variants |
| **acme-dns** (CNAME delegation) | *any* provider, by delegating `_acme-challenge` to a small dedicated server |

Tier 2 is the highest leverage per line of code in the whole list: three adapters, and the
"provider not supported" refusal stops being the common case.

**Tier 3 — frequently requested long tail**

Hetzner, Vultr, Gandi, OVH, Porkbun, deSEC, ClouDNS, NS1, DNSimple, Scaleway, Linode
(Akamai), DuckDNS, Dynu, CloudXNS, INWX, Netcup, TransIP, IBM Cloud, Oracle Cloud, Yandex,
西部数码 / 新网 / 22.cn.

**Tier 4 — the remainder of lego's ~180.**

### 4.2 Three ways to close the gap

| Strategy | Coverage | Cost | Notes |
|---|---|---|---|
| **A. Native adapters, wave by wave** | grows by 1–3 per wave | ~400–700 lines per family (adapter + stub-server tests) plus the surfaces in §2; signed providers need canonical-request care | No new runtime dependency; full control of error mapping; the pattern in `dns_cloudflare.rs` is the template |
| **B. lego sidecar presenter** | ~180 at once | one Go binary to deploy; a `provider + env config` surface (and therefore DDL, since the config must be stored somewhere) | This is how most of the industry actually does it (lego `serve`, acme.sh hooks). Fastest path to literal "all providers" |
| **C. Generic escape hatches** | the long tail, no sidecar | one `httpreq`-style presenter configured by URL/method/headers/body template, plus RFC 2136 | **the `httpreq` half has shipped** as the `HTTP_REQUEST` family (`dns_http_request.rs`): HTTPS-only endpoints, `{{recordValue}}` templating, secrets referenced by name from `vars`, and an allowlist-by-construction because there is no config surface beyond the operator's own document. RFC 2136 is still open |

These are not exclusive: A + C is the usual end state (native for the mainstream, escape
hatches for everything else). B is the only option that reaches "all" in one step.

## 5. Adding a family — the checklist

Adding a native family touches all four surfaces in §2. In order:

1. `dns_provider` in `sdkwork-deploy-cloud-account-port`: add the constant to `ALL`, plus its
   `vendor_code_for`, `family_for_vendor_code` aliases and `credential_kind_for`.
2. `DnsProviderKind` in the ACME crate: add the variant to the enum, `ALL`, `as_str`,
   `parse`, and write the adapter (copy `dns_cloudflare.rs` as the template; it carries a
   stub-server test for publish, withdraw, zone resolution and error handling).
3. Export it from the ACME crate's `lib.rs`, add the credential variant to
   `DeployDnsProviderCredential` (`kind`, `from_account_credential`, `from_secret_payload`)
   and wire `build_dns01_presenter`.
4. Contracts: add the family to the three **closed** enums in `apis/app-api/deploy/openapi.yaml`,
   run the materialiser, then regenerate the SDK.
5. Console: `CLOUD_ACCOUNT_CREDENTIAL_FIELDS` and the i18n labels are compile-forced once the
   SDK is regenerated; add the family to `DNS_FAMILIES` and `DNS_PROVIDER_SUGGESTIONS`, and
   its display name to `dnsFamilyLabel`.
6. Run
   `cargo test -p sdkwork-intelligence-deploy-repository-sqlx --test dns_provider_parity`
   **and** `--test dns_provider_reachability` — the first fails on whichever of the above is
   still missing, the second on a family that is spelled everywhere and reachable nowhere.
7. If the family has a read-only call that proves an account, override `verify_account` and
   classify its refusals per §1.1. Implement it in the adapter, where the vendor's codes live,
   and add one test per class: the refusals that must stop an order, and the ones that must
   not. A family that cannot be probed needs no work — the default is `Unsupported`, which is
   reported honestly and never blocks an order.

No DDL change is needed (§3).
