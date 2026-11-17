import type { AppResourceLinkResponse } from './app-resource-link-response';

export interface AppsResourceLinksUpdateResponse {
  code: 0;
  data: unknown & { item: AppResourceLinkResponse; };
  /** Server-owned request correlation id. */
  traceId: string;
}
