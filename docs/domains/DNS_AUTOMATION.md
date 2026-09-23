# DNS Automation

What the control plane writes into a customer's DNS zone, through which account,
and what it deliberately leaves alone.

This document is the operator-facing companion to
`ADR-20260723-managed-domain-tls-control-plane.md` §3 and §5 ("provider ports keep
CA, **DNS automation**, and secret storage replaceable") and
`ADR-20260917-dns-credentials-from-iam-cloud-account-center.md` (where the
credential comes from). It describes behaviour, not decisions.

## 1. Which records this deployment writes

Exactly one record per hostname is derived from the platform's own data:

| Record | Name | Value | Written when |
| --- | --- | --- | --- |
| Ownership proof | `_sdkwork-verification.<hostname>` (TXT) | `base64url(sha256(attempt id))` | the hostname is created, or its ownership is checked |
| ACME DNS-01 | `_acme-challenge.<identifier>` (TXT) | `base64url(sha256(key authorization))` | a certificate order that selected DNS-01 is presented |

Both are *platform-owned statements*: their names and values follow from data the
control plane already holds, so writing them cannot invent an address for
anything.

**Service records are not written here.** A bare hostname has no target until an
application binding names one, and the platform's canonical target for a custom
hostname is the app's own publishing hostname
(`<label>.app[-<env>].<suffix>`, see `sdkwork-deploy-core::app_domains`). That
target belongs to the binding flow, which knows the app; this layer only ever
writes the records the control plane can prove it owns.

## 2. Which account writes them

Resolution is a fallthrough, and the order is the operator's intent first:

1. the account pinned on the **certificate** (DNS-01 only — a bare ownership claim
   has no certificate behind it, so this step is skipped for verification);
2. the account pinned on the **zone** (`deploy_dns_zone.provider_account_id`),
   which is what the console sets when a root domain is bound to a cloud account;
3. the account the account center picks for the zone's declared `dns_provider`;
4. the deployment-level credential (`SDKWORK_DEPLOY_DNS_*`).

A step whose account is missing, disabled, credential-less, or contradicted by
the zone's declaration hands over to the next one. `None` at the end of the chain
is a normal answer: it means this hostname's records are published by hand, which
is the pre-existing behaviour and is unchanged byte for byte.

A zone is reachable through a hostname only once that hostname has a
`deploy_domain` row — the lookup joins it — which is why every write below
resolves *after* the row exists, or *before* it is removed.

## 3. Lifecycle

| Console action | Provider action | Ledger |
| --- | --- | --- |
| create a subdomain | publish the ownership TXT | recorded |
| create a root domain bound to an account | publish the apex ownership TXT | recorded |
| pin an account on an existing root domain | publish the apex ownership TXT | recorded |
| rename a subdomain | withdraw the old name's record, publish the new one's | replaced |
| delete a subdomain | withdraw the record | left on the soft-deleted row |
| delete a root domain | withdraw the apex record | left on the soft-deleted row |
| verify ownership | publish, then read DNS back | recorded |
| verify the root domain | publish the apex record, probe the account, read DNS back | recorded |

"The ledger" is the block under `deploy_domain.metadata.dnsOwnershipRecord`:
zone apex, record name, record value, and the provider's own record id when it
returned one. It exists because withdrawal needs more than the record name —
Cloudflare deletes by the provider's id, and a rename expires the verification
attempt, so the value cannot be re-derived from the attempt row afterwards
either. The publish response is the only place that knowledge exists, and it is
recorded at the moment it is obtained.

It lives in `metadata` rather than a column of its own because the baseline DDL
reaches existing databases through migration; the block is additive, and a row
that never had a record simply has no key.

### 3.1 Verifying the root domain itself

The console's root-domain table has its own 验证 action (`POST
/app/v3/api/domain_zones/{zoneId}/verify`, `domainZones.verify`). The zone keeps
no verification state of its own: ownership of `example.com` is ownership of its
apex hostname, and the zone ships with that hostname's `deploy_domain` row. The
operation is therefore the per-hostname pass on the apex row — the same
challenge (`_sdkwork-verification.<apex>`), the same auto-publish through the
zone's account, the same DNS lookup — with two additions:

- **A registration probe.** `Dns01Presenter::verify_account` is a read-only
  call that proves the credential works *and* that the provider holds this
  zone — the "域名确实注册在该服务商名下" fact a TXT lookup cannot supply. Its
  outcome travels in the response (`providerAccountChecked` /
  `providerAccountVerified` / `providerAccountRefusal`) and never gates the TXT
  path: a provider outage or an unprobeable family degrades the *answer*, not
  the verification.
- **The zone's post-pass status.** The response carries the apex row's
  verification state after the pass (`verificationStatus`), and the zone
  inventory projects the same row (`DomainZoneResponse.verificationStatus` /
  `verifiedAt`), so the console badge and the verify action read one fact.

A record already in place — published by an earlier pass or by the operator's
hand — verifies on the call without being republished, and a second check
re-derives the same value rather than invalidating what is out there. Deleting
the zone withdraws the apex record the automation published, exactly as a
hostname delete does for its own.

## 4. Consistency claims

These are the invariants the integration suite asserts, all against the
provider's own record set rather than against a return value:

- creating a subdomain in a bound zone publishes **exactly one** value for
  `_sdkwork-verification.<hostname>`, and the ledger describes the same value the
  provider was given;
- renaming moves the record: the new name carries a value and the old name
  carries none, so the provider holds exactly the renamed hostname's record;
- deleting withdraws **only** the deleted hostname's record — a neighbour's
  record in the same zone survives;
- a zone with no account publishes nothing on create, on delete, or on a failed
  credential, and both operations still succeed.

## 5. Failure policy

Every provider failure is **logged and swallowed**, never returned to the caller:

- a publish that fails leaves the operator on the manual path — the verification
  response still carries the record name and the value to publish;
- a withdrawal that fails leaves the record in place and says so in the log,
  because a domain must not become unverifiable (or undeletable) because a
  provider is down or a key was revoked;
- a rename publishes the new name **even when** the old withdrawal failed.
  Skipping it would leave the live hostname unverifiable, which is worse than
  leaving one stale record under a name this deployment no longer answers for.

`PLAN-2026-0003` §8.3 states the same rule for ACME DNS-01: an adapter failure
downgrades to manual presentation instead of failing the request.

## 6. 微信公众号域名验证文件

微信公众号的 JS 接口安全域名 / 网页授权域名 / 业务域名要求在域名根路径可
访问一个形如 `MP_verify_xxxxx.txt` 的校验文件。这条链路横跨控制面与边缘：

- **控制面**（`/app/v3/api/domain_zones/{zoneId}/wechat_verification`）：
  操作员把微信生成的 `.txt` 原文上传到 zone 上；内容按字节原样存储于
  `deploy_dns_zone`，不做任何改写——微信按精确字节比对。
- **边缘**（sdkwork-webserver 网关）：standalone 形态下边缘与控制面共享
  进程与数据库，网关按请求主机名进程内读取该 zone 的文件，在**单段 `.txt`
  根路径**上按窄优先级原样应答（GET/HEAD，`no-store`）；任何不命中都放行
  给宿主应用，宿主自己的 `/x.txt` 不受影响。读取带主机名级 TTL 缓存
  （命中 60s / 未命中 10s）：上传后至多等 10 秒即可被爬虫取到。
- **自检**（`.../wechat_verification/check`）：按微信爬虫的视角取回
  `https://<apex>/<fileName>`（https 失败再试 http）并与保存内容逐字节
  比对。`reachable` 与 `matched` 同时为真时，操作员到公众平台点"完成验证"
  即可。

## 7. Where this is verified

- `crates/sdkwork-intelligence-deploy-repository-sqlx/tests/domain_ownership_automation.rs`
  — the end-to-end suite over real PostgreSQL, with the provider adapters
  swapped at the resolver's factory for a recording presenter.
- `crates/sdkwork-intelligence-deploy-service/src/domain_dns_sync.rs` — the
  ledger's own unit tests, including the refusal of a partial block.
