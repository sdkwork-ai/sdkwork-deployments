export interface ChannelReleasePublishCommandRequest {
  action: 'upload' | 'submit_audit' | 'release' | 'rollback' | 'gray' | 'modify_domains' | 'set_privacy';
  authorizationId: string;
  templateId?: string;
  providerVersion?: string;
  extJson?: Record<string, unknown>;
  targetVersion?: string;
  grayPercentage?: number;
  items?: Record<string, unknown>[];
  domainAction?: 'add' | 'set' | 'delete';
  domains?: string[];
  privacy?: Record<string, unknown>;
  providerAppId?: string;
  projectDir?: string;
  privateKeyPath?: string;
  robot?: number;
}
