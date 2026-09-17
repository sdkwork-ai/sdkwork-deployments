export interface CertificateChallengeResponse {
  id: string;
  tenantId: string;
  orderId: string;
  identifierId: string;
  hostname: string;
  challengeType: string;
  proofSha256: string;
  presentationRef?: string;
  /** DNS-01 TXT record owner. A wildcard identifier and its apex share one record name, so presentation order never affects validation. */
  dnsRecordName?: string;
  dnsRecordType?: 'TXT';
  dnsRecordValue?: string;
  presentationExpiresAt?: string;
  status: string;
  attemptCount: number;
  checkedAt?: string;
  validatedAt?: string;
  lastErrorCode?: string;
  createdAt: string;
  updatedAt: string;
  version: string;
}
