import type { TemplatePurchaseResponse } from './template-purchase-response';

export interface TemplatePurchasesCreateResponse201 {
  code: 0;
  data: unknown & { item: TemplatePurchaseResponse; };
  /** Server-owned request correlation id. */
  traceId: string;
}
