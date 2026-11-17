export interface AppSourceGitInput {
  repoProvider: 'GITHUB' | 'GITEE' | 'GITLAB' | 'SELF_HOSTED';
  /** https://, http://, or an scp-style git@host:path URL. Credentials must not be embedded. */
  repoUrl: string;
  /** Branch, tag or commit. Absent means the repository default branch. */
  repoRef?: string;
  /** Repository-relative directory the code lives in. Absent (or /, ., ./) means the repository root. Absolute paths and .. segments are refused. */
  subPath?: string;
  /** Reference to the stored clone credential. The secret itself never travels here. */
  credentialSecretRef?: string;
  /** Absent defaults to SHALLOW. */
  cloneMode?: 'FULL' | 'SHALLOW';
}
