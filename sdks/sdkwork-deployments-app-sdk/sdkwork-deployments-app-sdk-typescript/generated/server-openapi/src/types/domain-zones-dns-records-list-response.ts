import type { DomainDnsRecordResponse } from './domain-dns-record-response';
import type { PageInfo } from './page-info';

export interface DomainZonesDnsRecordsListResponse {
  code: 0;
  data: unknown & { items: DomainDnsRecordResponse[]; pageInfo: PageInfo; };
  /** Server-owned request correlation id. */
  traceId: string;
}
