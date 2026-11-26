/** Outcome of an operator's "retry the Drive archive" command. */
export interface CertificateArchiveRetryResult {
  /** `true` when this call re-armed the archive: the row was reset to `PENDING` and the certificate worker will upload on its next tick. The operation does not wait for the upload and does not return the Drive ids, so a client polls the certificate and watches `archiveStatus` reach `ARCHIVED`. `false` means there was nothing to retry — already `ARCHIVED`, or archiving is switched off on this deployment (`SKIPPED`) — the idempotent answer to a double click or a second operator, not an error. */
  accepted: boolean;
  /** The archive state as of this call, so the caller learns the outcome without a second read. `ARCHIVED` on an `accepted: false` answer means the archive was already done; `SKIPPED` means this deployment does not archive at all. */
  archiveStatus: 'NONE' | 'PENDING' | 'STORING' | 'ARCHIVED' | 'FAILED' | 'SKIPPED';
}
