export interface EnvironmentPromotionResponse {
  id: string;
  appId: string;
  environmentId: string;
  environmentKey: string;
  releaseId: string;
  releaseVersion: string;
  fromEnvironmentId?: string;
  fromEnvironmentKey?: string;
  promotedBy?: string;
  /** The second subject that approved an approval-gated promotion. Absent when the environment did not require approval. */
  approverUserId?: string;
  note?: string;
  createdAt: string;
}
