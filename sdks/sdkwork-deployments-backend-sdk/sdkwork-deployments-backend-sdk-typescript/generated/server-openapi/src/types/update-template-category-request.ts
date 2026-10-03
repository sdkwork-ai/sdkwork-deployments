export interface UpdateTemplateCategoryRequest {
  parentId?: string;
  displayName?: string;
  description?: string;
  sortOrder?: number;
  status?: 'ACTIVE' | 'DISABLED';
}
