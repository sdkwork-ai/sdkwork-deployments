import type { ApplicationScaffoldInstallResponse } from './application-scaffold-install-response';

export interface MarketplaceScaffoldsInstallResponse {
  code: 0;
  data: unknown & { item: ApplicationScaffoldInstallResponse; };
  /** Server-owned request correlation id. */
  traceId: string;
}
