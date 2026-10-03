import type { TemplatePurchaseResponse } from './template-purchase-response';

export interface TemplatePurchasesRevokeResponse {
  code: 0;
  data: unknown & { item: TemplatePurchaseResponse; };
  /** Server-owned request correlation id. */
  traceId: string;
}
