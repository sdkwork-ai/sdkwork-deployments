import type { AppRemixResponse } from './app-remix-response';

export interface AppsRemixRetrieveResponse {
  code: 0;
  data: unknown & { item: AppRemixResponse; };
  /** Server-owned request correlation id. */
  traceId: string;
}
