export interface DownloadItemUpsertRequest {
  platform?: 'windows' | 'macos' | 'linux' | 'android' | 'ios' | 'harmonyos' | 'docker' | 'helm' | 'generic';
  architecture?: 'x64' | 'arm64' | 'universal';
  label?: string;
  fileName?: string;
  version?: string;
  releaseTag?: string;
  packageId?: string;
  artifactId?: string;
  downloadSource?: Record<string, unknown>;
  sizeBytes?: number;
  checksumSha256?: string;
  primaryAction?: boolean;
  disabled?: boolean;
  unavailableLabel?: string;
  sortOrder?: number;
}
