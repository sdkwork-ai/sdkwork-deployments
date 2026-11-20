import type { PageInfo } from './page-info';
import type { ScaffoldCategoryResponse } from './scaffold-category-response';

export interface ScaffoldsCategoriesListResponse {
  code: 0;
  data: unknown & { items: ScaffoldCategoryResponse[]; pageInfo: PageInfo; };
  /** Server-owned request correlation id. */
  traceId: string;
}
