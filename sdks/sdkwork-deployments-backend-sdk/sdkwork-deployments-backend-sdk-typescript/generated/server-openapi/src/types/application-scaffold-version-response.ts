export interface ApplicationScaffoldVersionResponse {
  id: string;
  scaffoldUuid: string;
  templateVersion: string;
  changelog: string;
  artifactUuid?: string;
  sourceAppVersion?: string;
  platformTargets: string[];
  packageSizeBytes: string;
  checksumSha256?: string;
  status: 'DRAFT' | 'PUBLISHED' | 'WITHDRAWN';
  publishedAt?: string;
  createdAt: string;
  updatedAt: string;
  version: string;
}
