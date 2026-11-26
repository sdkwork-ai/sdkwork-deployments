import type { CertificateArchiveRetryResult } from './certificate-archive-retry-result';

export interface CertificatesArchiveRetryResponse {
  code: 0;
  data: unknown & { item: CertificateArchiveRetryResult; };
  /** Server-owned request correlation id. */
  traceId: string;
}
