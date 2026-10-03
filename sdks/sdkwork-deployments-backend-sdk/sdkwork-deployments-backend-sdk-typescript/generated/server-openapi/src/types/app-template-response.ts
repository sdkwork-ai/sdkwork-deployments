export interface AppTemplateResponse {
  id: string;
  templateKey: string;
  displayName: string;
  summary: string;
  description: string;
  appUuid: string;
  categoryUuid: string;
  authorUserId: string;
  iconMediaRef?: string;
  coverMediaRef?: string;
  visibility: 'PUBLIC' | 'PRIVATE';
  pricingModel: 'FREE' | 'PAID';
  priceMinor: string;
  currency: string;
  status: 'DRAFT' | 'PENDING_REVIEW' | 'PUBLISHED' | 'REJECTED' | 'DISABLED';
  reviewNote?: string;
  isFeatured: boolean;
  installCount: string;
  viewCount: string;
  latestVersionUuid?: string;
  createdAt: string;
  updatedAt: string;
  version: string;
}
