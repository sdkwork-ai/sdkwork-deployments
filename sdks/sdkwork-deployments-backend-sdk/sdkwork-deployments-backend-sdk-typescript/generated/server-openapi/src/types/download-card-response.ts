import type { DownloadItemResponse } from './download-item-response';

export interface DownloadCardResponse {
  id: string;
  cardKey: string;
  title: string;
  description?: string;
  icon?: string;
  tone?: string;
  kind?: string;
  badge?: string;
  primaryStrategy: 'detected-platform' | 'first-available';
  hidden: boolean;
  sortOrder: number;
  items?: DownloadItemResponse[];
}
