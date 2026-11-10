export interface DownloadUpdateUpsertRequest {
  enabled?: boolean;
  provider?: 'deployments' | 'generic' | 'github' | 'none';
  feedUrl?: string;
  githubOwner?: string;
  githubRepo?: string;
  channelId?: string;
  channelKey?: 'nightly' | 'stable';
  channelFile?: string;
  allowedChannels?: ('nightly' | 'stable')[];
  allowPrerelease?: boolean;
  allowDowngrade?: boolean;
  minimumSupportedVersion?: string;
  checkIntervalMinutes?: number;
  rolloutPercentage?: number;
  releaseNotesUrl?: string;
}
