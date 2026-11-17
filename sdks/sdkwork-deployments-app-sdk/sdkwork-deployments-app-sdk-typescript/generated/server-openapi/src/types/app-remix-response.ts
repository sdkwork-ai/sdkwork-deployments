import type { AppRemixDownloadResponse } from './app-remix-download-response';

export interface AppRemixResponse {
  appId: string;
  remixEnabled: boolean;
  sourceKind?: 'GIT' | 'ARCHIVE';
  readiness: 'READY' | 'MISSING';
  readinessReason?: 'SOURCE_NOT_CONFIGURED' | 'SOURCE_KIND_MISMATCH' | 'GIT_REPOSITORY_UNUSABLE' | 'GIT_PROVIDER_UNSUPPORTED' | 'GIT_REPOSITORY_URL_INVALID' | 'GIT_REPOSITORY_NOT_VALIDATED' | 'GIT_REFERENCE_INVALID' | 'GIT_SUB_PATH_INVALID' | 'ARCHIVE_UNAVAILABLE';
  download?: AppRemixDownloadResponse;
  updatedAt?: string;
  version?: string;
}
