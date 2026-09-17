import type { CertificateMaterialPayload } from './certificate-material-payload';

export interface StoreCertificateVersionRequest {
  orderId: string;
  versionNo: string;
  serialSha256: string;
  fingerprintSha256: string;
  spkiSha256: string;
  chainSha256: string;
  issuer: string;
  subject: string;
  keyAlgorithm: 'RSA' | 'ECDSA';
  notBefore: string;
  notAfter: string;
  secretBundleRef: string;
  material: CertificateMaterialPayload;
}
