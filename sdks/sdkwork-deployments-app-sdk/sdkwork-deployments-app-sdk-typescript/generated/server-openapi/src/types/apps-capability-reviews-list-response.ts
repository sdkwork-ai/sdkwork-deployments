import type { CapabilityReviewResponse } from './capability-review-response';
import type { PageInfo } from './page-info';

export interface AppsCapabilityReviewsListResponse {
  code: 0;
  data: unknown & { items: CapabilityReviewResponse[]; pageInfo: PageInfo; };
  /** Server-owned request correlation id. */
  traceId: string;
}
