import type { DownloadCardResponse } from './download-card-response';

export interface DownloadCatalogResponse {
  id: string;
  appId?: string;
  catalogKey: string;
  productKey: string;
  productName: string;
  productVersion: string;
  defaultChannelKey: 'nightly' | 'stable';
  status: 'DRAFT' | 'PUBLISHED' | 'ARCHIVED';
  revision: string;
  publishedAt?: string;
  createdAt: string;
  updatedAt: string;
  version: string;
  cards?: DownloadCardResponse[];
}
