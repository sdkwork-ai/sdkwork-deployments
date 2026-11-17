import type { AppSourceArchiveResponse } from './app-source-archive-response';
import type { AppSourceGitResponse } from './app-source-git-response';

export interface AppSourceResponse {
  appId: string;
  remixEnabled: boolean;
  sourceKind?: 'GIT' | 'ARCHIVE';
  /** Derived on every read from the referenced repository or artifact; never stored. */
  readiness: 'READY' | 'MISSING';
  readinessReason?: 'SOURCE_NOT_CONFIGURED' | 'SOURCE_KIND_MISMATCH' | 'GIT_REPOSITORY_UNUSABLE' | 'GIT_PROVIDER_UNSUPPORTED' | 'GIT_REPOSITORY_URL_INVALID' | 'GIT_REPOSITORY_NOT_VALIDATED' | 'GIT_REFERENCE_INVALID' | 'GIT_SUB_PATH_INVALID' | 'ARCHIVE_UNAVAILABLE';
  git?: AppSourceGitResponse;
  archive?: AppSourceArchiveResponse;
  updatedAt?: string;
  version?: string;
}
