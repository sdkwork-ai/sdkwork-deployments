export interface DownloadCardUpsertRequest {
  title?: string;
  description?: string;
  icon?: string;
  tone?: string;
  kind?: string;
  badge?: string;
  primaryStrategy?: 'detected-platform' | 'first-available';
  hidden?: boolean;
  sortOrder?: number;
}
