import type { AppResourceLinkResponse } from './app-resource-link-response';
import type { PageInfo } from './page-info';

export interface AppsResourceLinksListResponse {
  code: 0;
  data: unknown & { items: AppResourceLinkResponse[]; pageInfo: PageInfo; };
  /** Server-owned request correlation id. */
  traceId: string;
}
