import type { ChannelReleaseResponse } from './channel-release-response';

export interface ChannelReleasesUpdateResponse {
  code: 0;
  data: unknown & { item: ChannelReleaseResponse; };
  /** Server-owned request correlation id. */
  traceId: string;
}
