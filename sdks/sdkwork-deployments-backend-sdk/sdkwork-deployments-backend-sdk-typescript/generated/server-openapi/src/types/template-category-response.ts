export interface TemplateCategoryResponse {
  id: string;
  categoryKey: string;
  parentId?: string;
  displayName: string;
  description?: string;
  sortOrder: number;
  status: 'ACTIVE' | 'DISABLED';
  createdAt: string;
  updatedAt: string;
  version: string;
}
