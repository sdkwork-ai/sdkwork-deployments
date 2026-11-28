import type { CapabilityDecisionCode } from './capability-decision-code';

export interface DecideCapabilityReviewRequest {
  decision: 'approve' | 'reject';
  decisionCode?: CapabilityDecisionCode;
  decisionNote?: string;
}
