import type { PageInfo } from './page-info';
import type { TemplatePurchaseResponse } from './template-purchase-response';

export interface TemplatePurchasesListResponse {
  code: 0;
  data: unknown & { items: TemplatePurchaseResponse[]; pageInfo: PageInfo; };
  /** Server-owned request correlation id. */
  traceId: string;
}
