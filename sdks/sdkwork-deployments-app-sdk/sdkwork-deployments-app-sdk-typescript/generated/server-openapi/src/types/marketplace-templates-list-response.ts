import type { AppTemplateSummaryResponse } from './app-template-summary-response';
import type { PageInfo } from './page-info';

export interface MarketplaceTemplatesListResponse {
  code: 0;
  data: unknown & { items: AppTemplateSummaryResponse[]; pageInfo: PageInfo; };
  /** Server-owned request correlation id. */
  traceId: string;
}
