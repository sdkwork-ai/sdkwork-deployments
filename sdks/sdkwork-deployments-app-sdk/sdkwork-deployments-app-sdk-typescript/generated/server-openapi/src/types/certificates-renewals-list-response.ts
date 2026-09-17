import type { CertificateRenewalResponse } from './certificate-renewal-response';
import type { PageInfo } from './page-info';

export interface CertificatesRenewalsListResponse {
  code: 0;
  data: unknown & { items: CertificateRenewalResponse[]; pageInfo: PageInfo; };
  /** Server-owned request correlation id. */
  traceId: string;
}
