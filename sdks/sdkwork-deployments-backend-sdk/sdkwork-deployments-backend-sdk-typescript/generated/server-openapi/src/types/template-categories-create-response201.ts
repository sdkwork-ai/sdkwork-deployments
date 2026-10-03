import type { TemplateCategoryResponse } from './template-category-response';

export interface TemplateCategoriesCreateResponse201 {
  code: 0;
  data: unknown & { item: TemplateCategoryResponse; };
  /** Server-owned request correlation id. */
  traceId: string;
}
