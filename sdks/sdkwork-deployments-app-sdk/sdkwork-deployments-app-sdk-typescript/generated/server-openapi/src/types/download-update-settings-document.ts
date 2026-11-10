export interface DownloadUpdateSettingsDocument {
  enabled: boolean;
  provider: 'deployments' | 'generic' | 'github' | 'none';
  feedUrl?: string;
  githubOwner?: string;
  githubRepo?: string;
  channel: string;
  channelFile?: string;
  allowedChannels: string[];
  allowPrerelease: boolean;
  allowDowngrade: boolean;
  minimumSupportedVersion?: string;
  checkIntervalMinutes: number;
  rolloutPercentage: number;
  releaseNotesUrl?: string;
}
