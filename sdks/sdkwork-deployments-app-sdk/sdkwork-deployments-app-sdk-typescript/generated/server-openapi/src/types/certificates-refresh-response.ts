import type { CertificateResponse } from './certificate-response';

export interface CertificatesRefreshResponse {
  code: 0;
  data: unknown & { item: CertificateResponse; };
  /** Server-owned request correlation id. */
  traceId: string;
}
