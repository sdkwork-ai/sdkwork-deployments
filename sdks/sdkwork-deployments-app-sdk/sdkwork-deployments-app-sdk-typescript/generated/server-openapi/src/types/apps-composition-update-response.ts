import type { AppCompositionResponse } from './app-composition-response';

export interface AppsCompositionUpdateResponse {
  code: 0;
  data: unknown & { item: AppCompositionResponse; };
  /** Server-owned request correlation id. */
  traceId: string;
}
