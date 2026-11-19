import type { AppTemplateInstallResponse } from './app-template-install-response';

export interface MarketplaceTemplatesInstallResponse201 {
  code: 0;
  data: unknown & { item: AppTemplateInstallResponse; };
  /** Server-owned request correlation id. */
  traceId: string;
}
