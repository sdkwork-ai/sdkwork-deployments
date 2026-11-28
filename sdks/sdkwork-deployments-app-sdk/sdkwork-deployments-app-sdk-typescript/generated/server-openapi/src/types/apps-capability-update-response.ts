import type { AppCapabilityResponse } from './app-capability-response';

export interface AppsCapabilityUpdateResponse {
  code: 0;
  data: unknown & { item: AppCapabilityResponse; };
  /** Server-owned request correlation id. */
  traceId: string;
}
