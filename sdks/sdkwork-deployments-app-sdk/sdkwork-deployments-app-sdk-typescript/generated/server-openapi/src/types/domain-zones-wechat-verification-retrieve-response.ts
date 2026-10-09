import type { DomainWechatVerificationResponse } from './domain-wechat-verification-response';

export interface DomainZonesWechatVerificationRetrieveResponse {
  code: 0;
  data: unknown & { item: DomainWechatVerificationResponse; };
  /** Server-owned request correlation id. */
  traceId: string;
}
