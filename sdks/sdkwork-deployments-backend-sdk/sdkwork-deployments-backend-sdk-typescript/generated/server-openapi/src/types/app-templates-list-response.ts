import type { AppTemplateResponse } from './app-template-response';
import type { PageInfo } from './page-info';

export interface AppTemplatesListResponse {
  code: 0;
  data: unknown & { items: AppTemplateResponse[]; pageInfo: PageInfo; };
  /** Server-owned request correlation id. */
  traceId: string;
}
