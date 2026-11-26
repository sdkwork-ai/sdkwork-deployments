/** One manual DNS-01 record an operator has to publish. The wire shape of the instruction the console renders: the exact TXT owner and value, the instant the CA stops accepting it, and the challenge's own state. The value is a public ACME proof, so echoing it exposes nothing DNS itself does not. */
export interface CertificateChallengePresentationResponse {
  /** The hostname this challenge authorizes. */
  identifier: string;
  /** The TXT owner to publish, e.g. `_acme-challenge.www.example.com`. Absent when the challenge has no presentation yet. */
  recordName?: string;
  /** The record type to publish. Present exactly when `recordName` is. */
  recordType?: 'TXT';
  /** The TXT value the CA expects. Compared exactly by the platform: case, padding and interior spacing are part of the value. */
  recordValue?: string;
  /** When the CA stops accepting this challenge value. */
  expiresAt?: string;
  /** The challenge's own state, e.g. `PENDING`, `PROCESSING`, `VALID`. */
  status: string;
}
