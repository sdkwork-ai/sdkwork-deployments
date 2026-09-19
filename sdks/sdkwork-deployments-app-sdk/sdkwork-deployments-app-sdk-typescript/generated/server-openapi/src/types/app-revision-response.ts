export interface AppRevisionResponse {
  id: string;
  number: string;
  descriptorSha256: string;
  validationStatus: 'VALID' | 'INVALID';
}
