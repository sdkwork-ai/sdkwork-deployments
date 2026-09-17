import type { DomainHostnameClaimResponse } from './domain-hostname-claim-response';

export interface DomainZonesHostnameClaimsEnsureResponse {
  code: 0;
  data: unknown & { items: DomainHostnameClaimResponse[]; };
  /** Server-owned request correlation id. */
  traceId: string;
}
