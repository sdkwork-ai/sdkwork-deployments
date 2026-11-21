export interface ChannelReleaseResponse {
  id: string;
  appId: string;
  environment: 'development' | 'test' | 'staging' | 'demo' | 'production';
  channel: 'wechat_mini_program' | 'wechat_official_account';
  accessMode: 'self_managed' | 'operator_authorized' | 'external_provider';
  providerVersion?: string;
  templateId?: string;
  commitId?: string;
  auditId?: string;
  releaseStatus: 'DRAFT' | 'PROJECTED' | 'UPLOADED' | 'AUDIT_SUBMITTED' | 'IN_REVIEW' | 'APPROVED' | 'RELEASING' | 'RELEASED' | 'REJECTED' | 'WITHDRAWN' | 'DEFERRED' | 'FAILED' | 'ROLLED_BACK' | 'SUPERSEDED';
  grayPercentage: number;
  rollbackOfReleaseId?: string;
  errorCode?: string;
  submittedAt?: string;
  releasedAt?: string;
  createdAt: string;
  updatedAt: string;
  version: string;
}
