import type { CloudAccountResponse } from './cloud-account-response';
import type { PageInfo } from './page-info';

export interface CloudAccountsListResponse {
  code: 0;
  data: unknown & { items: CloudAccountResponse[]; pageInfo: PageInfo; };
  /** Server-owned request correlation id. */
  traceId: string;
}
