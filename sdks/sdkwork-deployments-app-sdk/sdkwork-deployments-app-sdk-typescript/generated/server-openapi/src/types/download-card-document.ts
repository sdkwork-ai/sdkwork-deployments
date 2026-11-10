import type { DownloadActionDocument } from './download-action-document';

export interface DownloadCardDocument {
  id: string;
  title: string;
  description: string;
  icon?: string;
  tone?: string;
  kind?: string;
  badge?: string;
  primaryActionStrategy: string;
  actions: DownloadActionDocument[];
}
