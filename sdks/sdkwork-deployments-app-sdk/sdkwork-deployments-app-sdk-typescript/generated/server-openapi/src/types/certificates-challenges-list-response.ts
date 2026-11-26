import type { CertificateChallengePresentationList } from './certificate-challenge-presentation-list';

export interface CertificatesChallengesListResponse {
  code: 0;
  data: unknown & { item: CertificateChallengePresentationList; };
  /** Server-owned request correlation id. */
  traceId: string;
}
