import type { AppTemplateVersionResponse } from './app-template-version-response';
import type { PageInfo } from './page-info';

export interface AppTemplateVersionsListResponse {
  code: 0;
  data: unknown & { items: AppTemplateVersionResponse[]; pageInfo: PageInfo; };
  /** Server-owned request correlation id. */
  traceId: string;
}
