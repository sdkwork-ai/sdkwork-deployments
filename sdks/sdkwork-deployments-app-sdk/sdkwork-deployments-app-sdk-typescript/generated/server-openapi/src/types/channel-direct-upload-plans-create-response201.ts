import type { ChannelDirectUploadPlan } from './channel-direct-upload-plan';

export interface ChannelDirectUploadPlansCreateResponse201 {
  code: 0;
  data: unknown & { item: ChannelDirectUploadPlan; };
  /** Server-owned request correlation id. */
  traceId: string;
}
