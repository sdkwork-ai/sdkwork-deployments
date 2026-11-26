/** Outcome of an operator's "release this presentation for validation" command. */
export interface CertificateChallengeReleaseResult {
  /** `true` when this call recorded the release. `false` means there was nothing left to release (already released, already finished, or no parked manual order) — a retry, not an error. HTTP 200 carries both, so clients must not treat `false` as a failure; only a `ProblemDetail` body is a failure. */
  accepted: boolean;
}
