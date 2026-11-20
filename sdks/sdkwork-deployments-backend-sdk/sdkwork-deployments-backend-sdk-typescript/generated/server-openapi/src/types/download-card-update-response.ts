import type { DownloadCardResponse } from './download-card-response';

export interface DownloadCardUpdateResponse {
  code: 0;
  data: unknown & DownloadCardResponse;
  /** Server-owned request correlation id. */
  traceId: string;
}
