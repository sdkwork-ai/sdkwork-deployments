/** The PEM files of one issued certificate. The digests above are re-derived from these bytes and any disagreement refuses the whole request, so the recorded metadata and the stored material cannot drift apart. */
export interface CertificateMaterialPayload {
  /** The certificate chain the CA returned, leaf first and then its intermediates — byte for byte what an ACME client stores as `fullchain.pem`. The control plane splits it into `cert.pem`, `chain.pem` and `fullchain.pem`, and hashes it as given for `chainSha256`. */
  certificateChainPem: string;
  /** The private key. Stored sealed under a data key wrapped by the custody master key; it is never persisted as plaintext and is not readable over any API. */
  privateKeyPem: string;
  /** The trust anchor the chain terminates at. Optional: most CAs omit it because the root belongs in the client's trust store. When absent the control plane resolves the anchor from the chain itself, or from its configured trust anchor bundle, and refuses the request if neither applies — it never stores an unanchored bundle. */
  rootPem?: string;
}
