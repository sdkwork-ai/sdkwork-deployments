export interface TemplatePurchaseResponse {
  id: string;
  templateUuid: string;
  versionUuid: string;
  buyerUserId: string;
  pricingModel: 'FREE' | 'PAID';
  priceMinor: string;
  currency: string;
  paymentRef?: string;
  status: 'PENDING' | 'ACTIVE' | 'REVOKED';
  createdAt: string;
  updatedAt: string;
  version: string;
}
