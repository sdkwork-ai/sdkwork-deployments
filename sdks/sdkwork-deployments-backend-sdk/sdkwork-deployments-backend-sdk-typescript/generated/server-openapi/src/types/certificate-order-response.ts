export interface CertificateOrderResponse {
  id: string;
  tenantId: string;
  certificateId: string;
  acmeAccountId: string;
  requestedVersionNo: string;
  requestSha256: string;
  idempotencyKey: string;
  externalOrderDigest?: string;
  status: string;
  attemptCount: number;
  lastErrorCode?: string;
  /** CAA pre-issuance decision. `UNAUTHORIZED_CA` blocks issuance; the decision is recorded so a refusal stays auditable without re-querying DNS. Always present together with `caaCheckedAt`. */
  caaDecision?: 'PERMITTED' | 'UNAUTHORIZED_CA' | 'LOOKUP_FAILED';
  caaCheckedAt?: string;
  deadlineAt: string;
  createdAt: string;
  updatedAt: string;
  version: string;
}
