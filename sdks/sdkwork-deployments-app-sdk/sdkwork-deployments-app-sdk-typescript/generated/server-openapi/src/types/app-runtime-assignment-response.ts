export interface AppRuntimeAssignmentResponse {
  targetId: string;
  assignmentId: string;
  generation: string;
  status: 'PENDING' | 'PUBLISHING' | 'PUBLISHED' | 'FAILED' | 'SUPERSEDED';
}
