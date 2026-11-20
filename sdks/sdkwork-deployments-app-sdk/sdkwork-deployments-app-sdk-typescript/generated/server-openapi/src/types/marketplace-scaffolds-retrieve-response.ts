import type { ApplicationScaffoldResponse } from './application-scaffold-response';

export interface MarketplaceScaffoldsRetrieveResponse {
  code: 0;
  data: unknown & { item: ApplicationScaffoldResponse; };
  /** Server-owned request correlation id. */
  traceId: string;
}
