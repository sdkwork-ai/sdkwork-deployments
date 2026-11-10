import type { DownloadUpdateResponse } from './download-update-response';

export interface DownloadUpdateUpsertResponse {
  code: 0;
  data: unknown & DownloadUpdateResponse;
  /** Server-owned request correlation id. */
  traceId: string;
}
