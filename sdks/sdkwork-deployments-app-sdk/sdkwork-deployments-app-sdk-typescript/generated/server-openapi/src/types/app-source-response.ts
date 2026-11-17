import type { AppSourceArchiveResponse } from './app-source-archive-response';
import type { AppSourceGitResponse } from './app-source-git-response';

export interface AppSourceResponse {
  appId: string;
  remixEnabled: boolean;
  sourceKind?: 'GIT' | 'ARCHIVE';
  /** Derived on every read from the referenced repository or artifact; never stored. */
  readiness: 'READY' | 'MISSING';
  /** Closed-set code explaining a MISSING verdict. GIT_REPOSITORY_PENDING means the repository was never probed (run apps.source.validate); GIT_REPOSITORY_INVALID means the last probe failed (git.lastErrorCode names the failure); GIT_REPOSITORY_NOT_VALIDATED covers a status this API does not write. */
  readinessReason?: 'SOURCE_NOT_CONFIGURED' | 'SOURCE_KIND_MISMATCH' | 'GIT_REPOSITORY_UNUSABLE' | 'GIT_PROVIDER_UNSUPPORTED' | 'GIT_REPOSITORY_URL_INVALID' | 'GIT_REPOSITORY_NOT_VALIDATED' | 'GIT_REPOSITORY_PENDING' | 'GIT_REPOSITORY_INVALID' | 'GIT_REFERENCE_INVALID' | 'GIT_SUB_PATH_INVALID' | 'ARCHIVE_UNAVAILABLE';
  git?: AppSourceGitResponse;
  archive?: AppSourceArchiveResponse;
  updatedAt?: string;
  version?: string;
}
