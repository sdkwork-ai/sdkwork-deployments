import type { ChannelReleaseResponse } from './channel-release-response';
import type { PageInfo } from './page-info';

export interface ChannelReleasesListResponse {
  code: 0;
  data: unknown & { items: ChannelReleaseResponse[]; pageInfo: PageInfo; };
  /** Server-owned request correlation id. */
  traceId: string;
}
