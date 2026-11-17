import type { AppResponse } from './app-response';
import type { PageInfo } from './page-info';

export interface AppsListResponse {
  code: 0;
  data: unknown & { items: AppResponse[]; pageInfo: PageInfo; };
  /** Server-owned request correlation id. */
  traceId: string;
}
