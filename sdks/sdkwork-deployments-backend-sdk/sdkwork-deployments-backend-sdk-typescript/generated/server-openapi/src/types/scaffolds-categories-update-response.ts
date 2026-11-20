import type { ScaffoldCategoryResponse } from './scaffold-category-response';

export interface ScaffoldsCategoriesUpdateResponse {
  code: 0;
  data: unknown & { item: ScaffoldCategoryResponse; };
  /** Server-owned request correlation id. */
  traceId: string;
}
