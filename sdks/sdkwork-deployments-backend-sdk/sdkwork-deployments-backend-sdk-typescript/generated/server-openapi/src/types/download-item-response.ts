import type { DownloadUpdateResponse } from './download-update-response';

export interface DownloadItemResponse {
  id: string;
  itemKey: string;
  platform: string;
  architecture: string;
  label: string;
  fileName?: string;
  version?: string;
  releaseTag?: string;
  packageId?: string;
  artifactId?: string;
  downloadSource?: Record<string, unknown>;
  sizeBytes?: string;
  checksumSha256?: string;
  primaryAction: boolean;
  disabled: boolean;
  unavailableLabel?: string;
  sortOrder: number;
  update?: DownloadUpdateResponse;
}
