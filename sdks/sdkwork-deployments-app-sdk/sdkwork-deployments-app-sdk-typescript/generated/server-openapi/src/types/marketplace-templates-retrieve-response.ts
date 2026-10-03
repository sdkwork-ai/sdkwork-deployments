import type { AppTemplateResponse } from './app-template-response';

export interface MarketplaceTemplatesRetrieveResponse {
  code: 0;
  data: unknown & { item: AppTemplateResponse; };
  /** Server-owned request correlation id. */
  traceId: string;
}
