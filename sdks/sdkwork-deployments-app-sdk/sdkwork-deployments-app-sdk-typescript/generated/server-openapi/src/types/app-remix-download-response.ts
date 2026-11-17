import type { AppSourceArchiveResponse } from './app-source-archive-response';
import type { AppSourceGitResponse } from './app-source-git-response';

export interface AppRemixDownloadResponse {
  mode: 'GIT' | 'ARCHIVE';
  git?: AppSourceGitResponse;
  archive?: AppSourceArchiveResponse;
}
