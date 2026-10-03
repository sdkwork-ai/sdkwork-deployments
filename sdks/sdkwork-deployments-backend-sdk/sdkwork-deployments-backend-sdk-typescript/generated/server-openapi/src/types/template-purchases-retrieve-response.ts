import type { TemplatePurchaseResponse } from './template-purchase-response';

export interface TemplatePurchasesRetrieveResponse {
  code: 0;
  data: unknown & { item: TemplatePurchaseResponse; };
  /** Server-owned request correlation id. */
  traceId: string;
}
