import type { AppSourceResponse } from './app-source-response';

export interface AppsSourceValidateResponse {
  code: 0;
  data: unknown & { item: AppSourceResponse; };
  /** Server-owned request correlation id. */
  traceId: string;
}
