export interface CreateScaffoldCategoryRequest {
  parentId?: string;
  categoryKey: string;
  displayName: string;
  description?: string;
  sortOrder?: number;
}
