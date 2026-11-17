import type { AppRemixResponse } from './app-remix-response';

export interface AppsRemixUpdateResponse {
  code: 0;
  data: unknown & { item: AppRemixResponse; };
  /** Server-owned request correlation id. */
  traceId: string;
}
