import type { CertificateChallengePresentationResponse } from './certificate-challenge-presentation-response';

/** The manual presentation of one certificate's current order. */
export interface CertificateChallengePresentationList {
  /** `MANAGED` or `MANUAL`. A managed certificate has no instructions to show, so `items` is empty; the mode is still reported so the console can tell "nothing to publish" apart from "not prepared yet". */
  presentationMode: 'MANAGED' | 'MANUAL';
  /** Whether the operator's release has already been recorded. `true` means the order is queued for CA validation and waiting on the CA, not on the operator. */
  presentationReleased: boolean;
  items: CertificateChallengePresentationResponse[];
}
