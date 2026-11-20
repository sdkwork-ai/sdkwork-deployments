export interface CreateApplicationScaffoldVersionRequest {
  version: string;
  changelog?: string;
  artifactUuid?: string;
  sourceAppVersion?: string;
  platformTargets?: string[];
  packageSizeBytes?: string;
  checksumSha256?: string;
}
