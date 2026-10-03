import type { CreateAppTemplateVersionRequest } from './create-app-template-version-request';

export interface CreateAppTemplateRequest {
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
  initialVersion?: CreateAppTemplateVersionRequest;
}
