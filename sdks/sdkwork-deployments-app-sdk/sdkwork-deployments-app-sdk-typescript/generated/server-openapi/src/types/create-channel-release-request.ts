export interface CreateChannelReleaseRequest {
  environment: 'development' | 'test' | 'staging' | 'demo' | 'production';
  resourceLinkId: string;
  channel: 'wechat_mini_program' | 'wechat_official_account';
  accessMode: 'self_managed' | 'operator_authorized' | 'external_provider';
  templateId?: string;
  providerVersion?: string;
  extJson?: Record<string, unknown>;
}
