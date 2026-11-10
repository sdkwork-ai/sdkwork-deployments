export interface DownloadCatalogUpsertRequest {
  appId?: string;
  productKey?: string;
  productName?: string;
  productVersion?: string;
  defaultChannelKey?: 'nightly' | 'stable';
}
