import type { CapabilityReviewResponse } from './capability-review-response';

export interface AppsCapabilitySubmitResponse {
  code: 0;
  data: unknown & { item: CapabilityReviewResponse; };
  /** Server-owned request correlation id. */
  traceId: string;
}
