import type { AppTemplateResponse } from './app-template-response';

export interface AppTemplatesCreateResponse201 {
  code: 0;
  data: unknown & { item: AppTemplateResponse; };
  /** Server-owned request correlation id. */
  traceId: string;
}
