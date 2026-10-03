import type { TemplateCategoryResponse } from './template-category-response';

export interface TemplateCategoriesRetrieveResponse {
  code: 0;
  data: unknown & { item: TemplateCategoryResponse; };
  /** Server-owned request correlation id. */
  traceId: string;
}
