export interface CreateCertificateRequest {
  certName: string;
  domainIds: string[];
  /** `SINGLE_DOMAIN` issues for the exact names supplied. `WILDCARD` additionally plans the apex of each wildcard, because a wildcard SAN does not cover its own apex, and forces DNS-01 validation. */
  certificateScope?: 'SINGLE_DOMAIN' | 'WILDCARD';
  /** Operator preference. `AUTO` resolves to HTTP_01 for a `SINGLE_DOMAIN` scope and always to DNS_01 for `WILDCARD`. `HTTP_01` is rejected for a `WILDCARD` scope. */
  validationMethod?: 'AUTO' | 'HTTP_01' | 'DNS_01';
  caProfile?: 'LETS_ENCRYPT_STAGING' | 'LETS_ENCRYPT_PRODUCTION';
  preferredKeyAlgorithm?: 'RSA' | 'ECDSA';
  /** Whether the control plane keeps this certificate renewed on its own. Defaults to true: asking for a managed certificate means asking for a name that stays covered, not for a reminder to renew it. */
  autoRenew?: boolean;
  /** Days before expiry at which renewal starts. Stored per certificate rather than read from the app binding's TLS policy, because one certificate may back several listeners with different policies and a renewal trigger has to resolve to exactly one window. The configured lead time is additionally floored at one third of the certificate's lifetime, so a value larger than a short-lived certificate's lifetime cannot make a fresh certificate immediately due. */
  renewBeforeDays?: number;
  /** Cloud account that presents this certificate's DNS-01 challenges, overriding the zone's pin so one zone can issue through several vendor accounts. Optional: omitting it leaves the certificate deferring to its zone, then to the account center, then to the deployment-level provider configuration, decided when an order runs. */
  providerAccountId?: string;
}
