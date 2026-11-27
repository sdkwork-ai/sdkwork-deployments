import type { CertificateArtifactResponse } from './certificate-artifact-response';

export interface CertificatesArtifactsRetrieveResponse {
  code: 0;
  data: unknown & { item: CertificateArtifactResponse; };
  /** Server-owned request correlation id. */
  traceId: string;
}
