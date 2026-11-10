import type { DownloadUpdateSettingsDocument } from './download-update-settings-document';

export interface DownloadUpdateConfigDocument {
  schemaVersion: string;
  catalogKey: string;
  itemKey: string;
  productKey: string;
  platform: string;
  architecture: string;
  version?: string;
  revision: string;
  update: DownloadUpdateSettingsDocument;
}
