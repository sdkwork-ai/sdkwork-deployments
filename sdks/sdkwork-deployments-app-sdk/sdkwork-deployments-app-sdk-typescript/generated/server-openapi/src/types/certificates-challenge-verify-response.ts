import type { CertificateChallengeReleaseResult } from './certificate-challenge-release-result';

export interface CertificatesChallengeVerifyResponse {
  code: 0;
  data: unknown & { item: CertificateChallengeReleaseResult; };
  /** Server-owned request correlation id. */
  traceId: string;
}
