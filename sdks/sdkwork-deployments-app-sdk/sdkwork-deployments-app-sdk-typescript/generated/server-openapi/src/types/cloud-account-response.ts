/** A DNS-capable cloud account, projected from the platform provider account center where its credential is held and rotated. Deploy never returns credential material here, only whether one is configured. `tenantGlobal` is the wire form of the 租户全局 / 我的账号 split and is derived from `scopeType`, so the two cannot disagree. */
export interface CloudAccountResponse {
  /** Account id in the provider account center. */
  id: string;
  accountCode: string;
  displayName: string;
  /** Cloud vendor, e.g. aliyun, tencent, cloudflare. The account center owns this vocabulary. */
  vendorCode: string;
  /** The DNS family this account drives, derived from `vendorCode`. Absent for a vendor that drives no supported family, which is why such an account is listed but cannot be bound. */
  dnsProvider?: 'ALIYUN_DNS' | 'DNSPOD' | 'CLOUDFLARE';
  scopeType: 'platform' | 'tenant' | 'user';
  /** True for `platform` and `tenant` accounts, which any member of the tenant may use; false for an account one user bound to themselves. */
  tenantGlobal: boolean;
  /** Set for `user`-scope accounts only. */
  ownerUserId?: string;
  /** Whether this account is its level's default for its vendor. */
  isDefault: boolean;
  status: 'active' | 'disabled' | 'archived';
  /** Whether an active credential exists behind the account. An account without one can be listed but not bound: it looks configured and is not. */
  credentialConfigured: boolean;
  /** Capabilities the account advertises. Empty means unspecified, which the account center treats as reusable for everything. */
  capabilityCodes?: string[];
  environment?: 'development' | 'sandbox' | 'production';
}
