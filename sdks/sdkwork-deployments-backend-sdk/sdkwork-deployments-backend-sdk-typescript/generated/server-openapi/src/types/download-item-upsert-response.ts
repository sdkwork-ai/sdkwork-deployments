import type { DownloadItemResponse } from './download-item-response';

export interface DownloadItemUpsertResponse {
  code: 0;
  data: unknown & DownloadItemResponse;
  /** Server-owned request correlation id. */
  traceId: string;
}
