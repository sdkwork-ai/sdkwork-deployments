/** The fields are the union of what the supported families need so the console renders one form: `accessKeyId` is the public half (Aliyun AccessKeyId, DNSPod LoginId) and is unused by Cloudflare, while `secretAccessKey` is always the secret half (Aliyun AccessKeySecret, DNSPod ApiToken, Cloudflare ApiToken). */
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
}
