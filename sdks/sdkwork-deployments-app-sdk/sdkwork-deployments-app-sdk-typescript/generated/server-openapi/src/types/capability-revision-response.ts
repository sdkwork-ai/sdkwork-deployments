export interface CapabilityRevisionResponse {
  id: string;
  appId: string;
  revisionNo: number;
  source: 'DRAFT' | 'REVIEW' | 'PUBLISH';
  sha256: string;
  schemaVersion: number;
  createdAt: string;
  createdBy?: string;
  snapshot?: Record<string, unknown>;
}
