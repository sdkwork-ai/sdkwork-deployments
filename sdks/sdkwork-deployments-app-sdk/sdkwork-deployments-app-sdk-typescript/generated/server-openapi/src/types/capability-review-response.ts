import type { CapabilityDecisionCode } from './capability-decision-code';
import type { CapabilityReviewStatus } from './capability-review-status';

export interface CapabilityReviewResponse {
  id: string;
  appId: string;
  revisionId: string;
  attemptNo: number;
  status: CapabilityReviewStatus;
  submittedBy: string;
  submittedAt: string;
  reviewerId?: string;
  decidedAt?: string;
  decisionCode?: CapabilityDecisionCode;
  decisionNote?: string;
  note?: string;
  version: string;
}
