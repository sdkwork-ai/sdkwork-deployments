export interface AppTemplateSummaryResponse {
  id: string;
  templateType: 'APP' | 'PPT' | 'VIDEO';
  templateKey: string;
  displayName: string;
  summary: string;
  categoryUuid: string;
  iconMediaRef?: string;
  coverMediaRef?: string;
  visibility: 'PUBLIC' | 'PRIVATE';
  pricingModel: 'FREE' | 'PAID';
  priceMinor: string;
  currency: string;
  status: 'DRAFT' | 'PENDING_REVIEW' | 'PUBLISHED' | 'REJECTED' | 'DISABLED';
  isFeatured: boolean;
  installCount: string;
  viewCount: string;
  latestVersionUuid?: string;
  updatedAt: string;
  version: string;
}
