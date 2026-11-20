export interface UpdateApplicationScaffoldAdminRequest {
  status?: 'PENDING_REVIEW' | 'PUBLISHED' | 'REJECTED' | 'DISABLED';
  reviewNote?: string;
  isFeatured?: boolean;
  visibility?: 'PUBLIC' | 'PRIVATE';
}
