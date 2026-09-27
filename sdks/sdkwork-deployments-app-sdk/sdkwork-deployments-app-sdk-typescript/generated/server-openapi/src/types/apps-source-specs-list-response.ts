import type { AppSourceSpecResponse } from './app-source-spec-response';
import type { PageInfo } from './page-info';

export interface AppsSourceSpecsListResponse {
  code: 0;
  data: unknown & { items: AppSourceSpecResponse[]; pageInfo: PageInfo; };
  /** Server-owned request correlation id. */
  traceId: string;
}
