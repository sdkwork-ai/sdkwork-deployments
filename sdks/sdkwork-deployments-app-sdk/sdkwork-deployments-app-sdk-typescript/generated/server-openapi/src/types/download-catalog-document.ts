import type { DownloadCardDocument } from './download-card-document';
import type { DownloadProductDocument } from './download-product-document';

export interface DownloadCatalogDocument {
  schemaVersion: string;
  generatedAt: string;
  product: DownloadProductDocument;
  cards: DownloadCardDocument[];
}
