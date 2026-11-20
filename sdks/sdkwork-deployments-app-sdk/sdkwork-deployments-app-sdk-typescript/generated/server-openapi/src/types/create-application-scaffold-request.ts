import type { ApplicationScaffoldScreenshot } from './application-scaffold-screenshot';
import type { CreateApplicationScaffoldVersionRequest } from './create-application-scaffold-version-request';

export interface CreateApplicationScaffoldRequest {
  appUuid: string;
  templateType?: 'APP' | 'PPT' | 'VIDEO';
  categoryUuid: string;
  templateKey: string;
  displayName: string;
  summary: string;
  description?: string;
  visibility?: 'PUBLIC' | 'PRIVATE';
  pricingModel?: 'FREE' | 'PAID';
  priceMinor?: string;
  currency?: string;
  iconMediaRef?: string;
  coverMediaRef?: string;
  screenshots?: ApplicationScaffoldScreenshot[];
  initialVersion?: CreateApplicationScaffoldVersionRequest;
}
