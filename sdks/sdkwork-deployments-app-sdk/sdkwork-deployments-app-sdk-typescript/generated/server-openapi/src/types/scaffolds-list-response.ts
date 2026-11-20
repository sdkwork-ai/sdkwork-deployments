import type { ApplicationScaffoldResponse } from './application-scaffold-response';
import type { PageInfo } from './page-info';

export interface ScaffoldsListResponse {
  code: 0;
  data: unknown & { items: ApplicationScaffoldResponse[]; pageInfo: PageInfo; };
  /** Server-owned request correlation id. */
  traceId: string;
}
