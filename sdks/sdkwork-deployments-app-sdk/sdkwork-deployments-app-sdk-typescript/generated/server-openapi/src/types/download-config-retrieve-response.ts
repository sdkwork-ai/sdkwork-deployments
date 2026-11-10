import type { DownloadUpdateConfigDocument } from './download-update-config-document';

export interface DownloadConfigRetrieveResponse {
  code: 0;
  data: unknown & DownloadUpdateConfigDocument;
  /** Server-owned request correlation id. */
  traceId: string;
}
