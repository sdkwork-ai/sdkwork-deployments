/** The credential halves the chosen family needs, so the console renders the vendor's own field names for that family rather than a union of every family's: `accessKeyId` is the public half (Aliyun AccessKeyId, DNSPod LoginId) and is absent for Cloudflare, while `secretAccessKey` is always the secret half (Aliyun AccessKeySecret, DNSPod ApiToken, Cloudflare ApiToken). A `dns_provider` that names no supported family is refused rather than stored, because a credential whose vendor does not match the domain it will publish for fails at the first order. */
export interface CreateCloudAccountRequest {
  displayName: string;
  /** The account code within its scope, pre-filled by the console with a suggestion rather than derived server-side: a derived code collides the second time a tenant adds a second account for the same vendor. */
  accountCode?: string;
  dnsProvider: 'ALIYUN_DNS' | 'DNSPOD' | 'CLOUDFLARE';
  /** A tenant member may not publish a `platform` account; the account center refuses it, and the console offers the level only where it can succeed. */
  scopeType?: 'platform' | 'tenant' | 'user';
  environment?: 'development' | 'sandbox' | 'production';
  /** Promote this account to its level's default for its vendor. */
  isDefault?: boolean;
  /** Required by the key-pair families, ignored by bearer-token ones. */
  accessKeyId?: string;
  secretAccessKey: string;
  sessionToken?: string;
  /** The operator's own statement that the credential is an active one for the declared vendor with permission to edit the zones it will publish into. The console cannot probe a credential — that would mean dispatching it to the vendor from the read path — so it asks the operator instead, and this is what records that they were asked. Refused when false rather than silently defaulted, so a caller cannot store an unattested credential by omission. */
  confirmsCredential?: boolean;
}
