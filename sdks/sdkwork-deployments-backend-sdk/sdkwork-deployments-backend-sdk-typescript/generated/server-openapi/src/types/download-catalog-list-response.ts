import type { DownloadCatalogSummaryResponse } from './download-catalog-summary-response';

export interface DownloadCatalogListResponse {
  code: 0;
  data: unknown & DownloadCatalogSummaryResponse[];
  /** Server-owned request correlation id. */
  traceId: string;
}
