import type { AppFactoryReadiness } from './app-factory-readiness';

export interface AppsReadinessRetrieveResponse {
  code: 0;
  data: unknown & { item: AppFactoryReadiness; };
  /** Server-owned request correlation id. */
  traceId: string;
}
