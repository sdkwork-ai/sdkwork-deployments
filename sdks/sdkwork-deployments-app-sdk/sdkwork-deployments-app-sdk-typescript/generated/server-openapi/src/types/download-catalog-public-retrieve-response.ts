import type { DownloadCatalogDocument } from './download-catalog-document';

export interface DownloadCatalogPublicRetrieveResponse {
  code: 0;
  data: unknown & DownloadCatalogDocument;
  /** Server-owned request correlation id. */
  traceId: string;
}
