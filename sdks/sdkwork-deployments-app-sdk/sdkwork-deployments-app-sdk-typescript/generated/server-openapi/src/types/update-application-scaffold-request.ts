import type { ApplicationScaffoldScreenshot } from './application-scaffold-screenshot';

export interface UpdateApplicationScaffoldRequest {
  displayName?: string;
  summary?: string;
  description?: string;
  categoryUuid?: string;
  visibility?: 'PUBLIC' | 'PRIVATE';
  pricingModel?: 'FREE' | 'PAID';
  priceMinor?: string;
  currency?: string;
  iconMediaRef?: string;
  coverMediaRef?: string;
  screenshots?: ApplicationScaffoldScreenshot[];
}
