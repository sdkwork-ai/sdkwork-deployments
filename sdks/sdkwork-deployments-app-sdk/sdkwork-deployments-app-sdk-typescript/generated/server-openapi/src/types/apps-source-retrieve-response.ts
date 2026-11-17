import type { AppSourceResponse } from './app-source-response';

export interface AppsSourceRetrieveResponse {
  code: 0;
  data: unknown & { item: AppSourceResponse; };
  /** Server-owned request correlation id. */
  traceId: string;
}
