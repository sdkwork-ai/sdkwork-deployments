import type { AppDomainResponse } from './app-domain-response';

export interface AppsDomainsListResponse {
  code: 0;
  data: unknown & { items: AppDomainResponse[]; };
  /** Server-owned request correlation id. */
  traceId: string;
}
