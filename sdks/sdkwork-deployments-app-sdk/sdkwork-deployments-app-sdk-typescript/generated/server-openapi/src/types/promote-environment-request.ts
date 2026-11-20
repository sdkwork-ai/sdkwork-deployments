export interface PromoteEnvironmentRequest {
  releaseId: string;
  fromEnvironmentId?: string;
  /** Confirms the promotion of an approval-gated environment. Not sufficient on its own: `approverUserId` must name a second subject. */
  approve?: boolean;
  /** The user subject that approves this promotion. Required when the target environment declares `approvalRequired`, and it must differ from the promoter. */
  approverUserId?: string;
  note?: string;
}
