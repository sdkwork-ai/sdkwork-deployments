export interface AppSourceGitResponse {
  repoProvider: string;
  repoUrl: string;
  /** The effective ref to check out; falls back to the repository default branch. */
  repoRef: string;
  subPath?: string;
  cloneMode: string;
  credentialSecretRef?: string;
  /** VALIDATED is what makes a Git source ready. A GIT write lands in PENDING; POST /app/v3/api/apps/{appId}/source/validate is the only operation that can record VALIDATED or INVALID. REVOKED and ARCHIVED exist in the shared repository table and are never written for a remix source; a row carrying one reads as MISSING with GIT_REPOSITORY_NOT_VALIDATED. */
  repoStatus: 'PENDING' | 'VALIDATED' | 'INVALID' | 'REVOKED' | 'ARCHIVED';
  /** When the repository last passed validation. Set exactly when repoStatus is VALIDATED, cleared otherwise. */
  lastValidatedAt?: string;
  /** Outcome of the last validation attempt. On a VALIDATED row it is present only when the repository is reachable but demands credentials (GIT_VALIDATION_CREDENTIALS_REQUIRED); on a PENDING row it explains why no probe ran; on an INVALID row it names the failure. */
  lastErrorCode?: 'GIT_VALIDATION_UNSUPPORTED_SCHEME' | 'GIT_VALIDATION_URL_INVALID' | 'GIT_VALIDATION_REPOSITORY_NOT_FOUND' | 'GIT_VALIDATION_SERVER_ERROR' | 'GIT_VALIDATION_UNEXPECTED_STATUS' | 'GIT_VALIDATION_TIMEOUT' | 'GIT_VALIDATION_NETWORK_FAILURE' | 'GIT_VALIDATION_CREDENTIALS_REQUIRED' | 'GIT_VALIDATION_PROBE_UNAVAILABLE';
  /** Operator-facing sentence for lastErrorCode, so a client does not have to carry its own copy of the vocabulary. */
  lastErrorDescription?: string;
  /** true when the last validation found the repository reachable but answering 401/403, so the clone needs credentialSecretRef. */
  credentialsRequired?: boolean;
}
