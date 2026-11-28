import type { CapabilityRevisionResponse } from './capability-revision-response';

export interface AppsCapabilityRevisionsRetrieveResponse {
  code: 0;
  data: unknown & { item: CapabilityRevisionResponse; };
  /** Server-owned request correlation id. */
  traceId: string;
}
