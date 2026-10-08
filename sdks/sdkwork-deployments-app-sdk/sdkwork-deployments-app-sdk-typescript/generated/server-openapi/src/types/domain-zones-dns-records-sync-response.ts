import type { DomainDnsSyncResponse } from './domain-dns-sync-response';

export interface DomainZonesDnsRecordsSyncResponse {
  code: 0;
  data: unknown & { item: DomainDnsSyncResponse; };
  /** Server-owned request correlation id. */
  traceId: string;
}
