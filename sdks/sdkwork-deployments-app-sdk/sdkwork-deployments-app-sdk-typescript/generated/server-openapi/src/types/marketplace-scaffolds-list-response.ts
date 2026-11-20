import type { ApplicationScaffoldSummaryResponse } from './application-scaffold-summary-response';
import type { PageInfo } from './page-info';

export interface MarketplaceScaffoldsListResponse {
  code: 0;
  data: unknown & { items: ApplicationScaffoldSummaryResponse[]; pageInfo: PageInfo; };
  /** Server-owned request correlation id. */
  traceId: string;
}
