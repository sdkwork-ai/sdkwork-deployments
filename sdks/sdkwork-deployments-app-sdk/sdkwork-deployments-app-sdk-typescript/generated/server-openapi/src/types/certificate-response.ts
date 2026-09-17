export interface CertificateResponse {
  id: string;
  certName: string;
  certificateSource: 'MANAGED' | 'CUSTOM';
  caProfile: 'LETS_ENCRYPT_STAGING' | 'LETS_ENCRYPT_PRODUCTION' | 'CUSTOM';
  certificateScope: 'SINGLE_DOMAIN' | 'WILDCARD';
  /** The resolved method the challenge orchestrator must present. A `WILDCARD` certificate always reports `DNS_01`. */
  validationMethod: 'AUTO' | 'HTTP_01' | 'DNS_01';
  /** The cloud account pinned to present this certificate's DNS-01 challenges, absent when the certificate defers to its zone or to the deployment-level provider configuration. */
  providerAccountId?: string;
  preferredKeyAlgorithm: 'RSA' | 'ECDSA';
  identifiers: string[];
  currentVersionId?: string;
  issuer?: string;
  notBefore?: string;
  notAfter?: string;
  autoRenew: boolean;
  renewalStatus: 'NONE' | 'PLANNED' | 'PROCESSING' | 'FAILED';
  status: 'PENDING' | 'ISSUING' | 'ACTIVE' | 'EXPIRED' | 'FAILED' | 'REVOKED';
  /** Lead time before expiry at which renewal starts. The server sends the derived instants below so every surface agrees on one window rule instead of each re-implementing it. */
  renewBeforeDays: number;
  /** When renewal work becomes due for the version being served: `max(notBefore + lifetime/3, notAfter - renewBeforeDays)`. The lifetime floor keeps a lead time longer than a short-lived certificate's own life from making a fresh certificate immediately due. */
  renewalDueAt?: string;
  /** Whole days until the served version expires, floored; negative once past. Floored so that a certificate with twelve hours left and one that expired yesterday cannot both render as `0`. */
  daysUntilExpiry?: number;
  /** Where the served version sits in its validity window right now. Computed at read time, never stored: a persisted phase would be wrong from the instant the clock crossed a boundary. */
  validityPhase?: 'NOT_YET_VALID' | 'VALID' | 'EXPIRING_SOON' | 'EXPIRED';
  /** When renewal last ran to completion, successfully or not. */
  lastRenewalAt?: string;
  /** Consecutive failed renewal attempts, reset to zero by a success. Exposed because the retry backoff hides repetition from the schedule: without it a certificate failing every day is indistinguishable from one that has never been tried. */
  renewalFailureCount: number;
  createdAt: string;
  updatedAt: string;
  version: string;
}
