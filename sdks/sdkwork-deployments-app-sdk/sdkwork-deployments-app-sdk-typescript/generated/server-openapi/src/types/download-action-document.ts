export interface DownloadActionDocument {
  id: string;
  label: string;
  platform: string;
  architecture: string;
  href: string;
  fileName?: string;
  version?: string;
  releaseTag?: string;
  sizeBytes?: string;
  sha256?: string;
  primary: boolean;
  disabled: boolean;
  unavailableLabel?: string;
}
