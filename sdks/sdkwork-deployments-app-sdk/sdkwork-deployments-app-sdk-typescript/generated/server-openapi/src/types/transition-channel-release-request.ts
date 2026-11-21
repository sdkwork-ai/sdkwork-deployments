export interface TransitionChannelReleaseRequest {
  expectedVersion: string;
  releaseStatus: 'DRAFT' | 'PROJECTED' | 'UPLOADED' | 'AUDIT_SUBMITTED' | 'IN_REVIEW' | 'APPROVED' | 'RELEASING' | 'RELEASED' | 'REJECTED' | 'WITHDRAWN' | 'DEFERRED' | 'FAILED' | 'ROLLED_BACK' | 'SUPERSEDED';
  commitId?: string;
  auditId?: string;
  errorCode?: string;
}
