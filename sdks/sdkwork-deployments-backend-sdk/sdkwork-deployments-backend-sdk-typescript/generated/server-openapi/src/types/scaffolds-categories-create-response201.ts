import type { ScaffoldCategoryResponse } from './scaffold-category-response';

export interface ScaffoldsCategoriesCreateResponse201 {
  code: 0;
  data: unknown & { item: ScaffoldCategoryResponse; };
  /** Server-owned request correlation id. */
  traceId: string;
}
