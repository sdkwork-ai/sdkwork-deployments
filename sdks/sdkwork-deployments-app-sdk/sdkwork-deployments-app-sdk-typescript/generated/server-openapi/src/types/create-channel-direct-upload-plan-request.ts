export interface CreateChannelDirectUploadPlanRequest {
  providerAppId: string;
  projectDir: string;
  privateKeyPath: string;
  version: string;
  description: string;
  robot?: number;
}
