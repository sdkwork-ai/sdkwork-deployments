export interface TemplatePurchaseResponse {
  id: string;
  templateUuid: string;
  versionUuid: string;
  buyerUserId: string;
  pricingModel: 'FREE' | 'PAID';
  priceMinor: string;
  currency: string;
  orderId?: string;
  orderNo?: string;
  status: 'ACTIVE' | 'REVOKED';
  createdAt: string;
  updatedAt: string;
  version: string;
}
