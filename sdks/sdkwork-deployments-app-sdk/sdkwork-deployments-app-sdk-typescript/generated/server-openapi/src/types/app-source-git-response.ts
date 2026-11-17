export interface AppSourceGitResponse {
  repoProvider: string;
  repoUrl: string;
  /** The effective ref to check out; falls back to the repository default branch. */
  repoRef: string;
  subPath?: string;
  cloneMode: string;
  credentialSecretRef?: string;
  /** VALIDATED is what makes a Git source ready. */
  repoStatus: string;
}
