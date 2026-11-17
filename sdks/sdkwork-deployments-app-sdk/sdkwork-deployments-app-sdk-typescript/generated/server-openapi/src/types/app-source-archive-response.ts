export interface AppSourceArchiveResponse {
  artifactId: string;
  uploadSessionId: string;
  packageType: number;
  fileName: string;
  contentType: string;
  contentLength: string;
  checksumSha256?: string;
  /** Drive node identity to resolve download bytes through the Drive app API. */
  driveNodeId: string;
  driveSpaceId?: string;
  createdAt: string;
}
