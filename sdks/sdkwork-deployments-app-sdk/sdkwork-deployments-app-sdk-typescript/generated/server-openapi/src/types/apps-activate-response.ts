import type { AppResponse } from './app-response';

export interface AppsActivateResponse {
  code: 0;
  data: unknown & { item: AppResponse; };
  /** Server-owned request correlation id. */
  traceId: string;
}
