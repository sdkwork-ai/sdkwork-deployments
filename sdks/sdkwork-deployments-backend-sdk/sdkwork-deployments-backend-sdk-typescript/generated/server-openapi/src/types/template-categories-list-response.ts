import type { PageInfo } from './page-info';
import type { TemplateCategoryResponse } from './template-category-response';

export interface TemplateCategoriesListResponse {
  code: 0;
  data: unknown & { items: TemplateCategoryResponse[]; pageInfo: PageInfo; };
  /** Server-owned request correlation id. */
  traceId: string;
}
