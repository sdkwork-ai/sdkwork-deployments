import type { EnvironmentPromotionResponse } from './environment-promotion-response';
import type { PageInfo } from './page-info';

export interface AppEnvironmentPromotionsListResponse {
  code: 0;
  data: unknown & { items: EnvironmentPromotionResponse[]; pageInfo: PageInfo; };
  /** Server-owned request correlation id. */
  traceId: string;
}
