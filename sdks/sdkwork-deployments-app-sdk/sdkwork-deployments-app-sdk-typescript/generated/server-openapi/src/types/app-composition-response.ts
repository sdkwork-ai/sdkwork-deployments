import type { AppRevisionResponse } from './app-revision-response';
import type { AppRuntimeAssignmentResponse } from './app-runtime-assignment-response';

export interface AppCompositionResponse {
  siteId: string;
  siteVersion: string;
  revision: AppRevisionResponse;
  runtimeAssignments: AppRuntimeAssignmentResponse[];
}
