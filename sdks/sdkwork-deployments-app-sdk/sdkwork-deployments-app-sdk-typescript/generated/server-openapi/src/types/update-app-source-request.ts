import type { AppSourceArchiveInput } from './app-source-archive-input';
import type { AppSourceGitInput } from './app-source-git-input';

export interface UpdateAppSourceRequest {
  /** Selects which half of the request is required. */
  kind: 'GIT' | 'ARCHIVE';
  git?: AppSourceGitInput;
  archive?: AppSourceArchiveInput;
}
