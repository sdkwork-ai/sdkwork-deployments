import type { ApplicationScaffoldVersionResponse } from './application-scaffold-version-response';
import type { PageInfo } from './page-info';

export interface ScaffoldsVersionsListResponse {
  code: 0;
  data: unknown & { items: ApplicationScaffoldVersionResponse[]; pageInfo: PageInfo; };
  /** Server-owned request correlation id. */
  traceId: string;
}
