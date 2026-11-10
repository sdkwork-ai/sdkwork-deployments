export interface DownloadCatalogSummaryResponse {
  id: string;
  catalogKey: string;
  productKey: string;
  productName: string;
  productVersion: string;
  status: 'DRAFT' | 'PUBLISHED' | 'ARCHIVED';
  revision: string;
  publishedAt?: string;
  updatedAt: string;
  version: string;
}
