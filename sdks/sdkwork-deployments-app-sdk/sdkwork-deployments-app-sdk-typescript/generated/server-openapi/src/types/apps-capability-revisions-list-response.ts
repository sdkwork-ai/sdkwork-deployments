import type { CapabilityRevisionResponse } from './capability-revision-response';
import type { PageInfo } from './page-info';

export interface AppsCapabilityRevisionsListResponse {
  code: 0;
  data: unknown & { items: CapabilityRevisionResponse[]; pageInfo: PageInfo; };
  /** Server-owned request correlation id. */
  traceId: string;
}
