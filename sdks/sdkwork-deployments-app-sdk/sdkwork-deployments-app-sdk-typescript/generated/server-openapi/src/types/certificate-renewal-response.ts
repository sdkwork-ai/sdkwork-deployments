/** One renewal attempt. The previous/new window pair is the point of the record: it is what makes continuous coverage auditable after the version it describes has been superseded. */
export interface CertificateRenewalResponse {
  id: string;
  certificateId: string;
  /** `SCHEDULED` came from the due-certificate sweep, `MANUAL` from an operator request. Recorded rather than inferred, because it changes how a failure reads: a scheduled one is broken automation that keeps retrying on its own, a manual one is an act the operator is already watching. */
  triggerKind: 'SCHEDULED' | 'MANUAL';
  /** `PLANNED` and `ORDERED` are open, and at most one attempt per certificate may be open; every other value is terminal and carries `finishedAt`. */
  status: 'PLANNED' | 'ORDERED' | 'SUCCEEDED' | 'FAILED' | 'SKIPPED' | 'CANCELLED';
  attemptNo: number;
  previousVersionId?: string;
  resultingVersionId?: string;
  previousNotBefore?: string;
  /** Expiry of the version being replaced, captured when the attempt was claimed, so the handover stays auditable even though that version is now superseded. */
  previousNotAfter?: string;
  newNotBefore?: string;
  newNotAfter?: string;
  scheduledAt: string;
  startedAt?: string;
  finishedAt?: string;
  lastErrorCode?: string;
  createdAt: string;
}
