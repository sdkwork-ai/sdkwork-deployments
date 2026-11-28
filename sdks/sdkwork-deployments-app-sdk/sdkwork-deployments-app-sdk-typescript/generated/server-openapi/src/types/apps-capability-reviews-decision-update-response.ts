import type { CapabilityReviewResponse } from './capability-review-response';

export interface AppsCapabilityReviewsDecisionUpdateResponse {
  code: 0;
  data: unknown & { item: CapabilityReviewResponse; };
  /** Server-owned request correlation id. */
  traceId: string;
}
