import type { HealthCheckResponse } from './health-check-response';

export interface AppsHealthChecksCreateResponse201 {
  code: 0;
  data: unknown & { item: HealthCheckResponse; };
  /** Server-owned request correlation id. */
  traceId: string;
}
