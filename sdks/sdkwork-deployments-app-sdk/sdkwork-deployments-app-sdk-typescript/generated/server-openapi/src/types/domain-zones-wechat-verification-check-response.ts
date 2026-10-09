import type { DomainWechatVerificationCheckResponse } from './domain-wechat-verification-check-response';

export interface DomainZonesWechatVerificationCheckResponse {
  code: 0;
  data: unknown & { item: DomainWechatVerificationCheckResponse; };
  /** Server-owned request correlation id. */
  traceId: string;
}
