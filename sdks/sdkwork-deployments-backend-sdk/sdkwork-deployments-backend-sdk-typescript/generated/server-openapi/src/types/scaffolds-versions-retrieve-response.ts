import type { ApplicationScaffoldVersionResponse } from './application-scaffold-version-response';

export interface ScaffoldsVersionsRetrieveResponse {
  code: 0;
  data: unknown & { item: ApplicationScaffoldVersionResponse; };
  /** Server-owned request correlation id. */
  traceId: string;
}
