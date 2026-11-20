import type { DownloadCatalogResponse } from './download-catalog-response';

export interface DownloadCatalogUpdateResponse {
  code: 0;
  data: unknown & DownloadCatalogResponse;
  /** Server-owned request correlation id. */
  traceId: string;
}
