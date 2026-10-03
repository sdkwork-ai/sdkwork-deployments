export interface CreateTemplateCategoryRequest {
  parentId?: string;
  categoryKey: string;
  displayName: string;
  description?: string;
  sortOrder?: number;
}
