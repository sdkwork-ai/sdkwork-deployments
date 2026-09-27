import type { AppSourceSpecResponse } from './app-source-spec-response';

export interface AppsSourceSpecsCreateResponse201 {
  code: 0;
  data: unknown & { item: AppSourceSpecResponse; };
  /** Server-owned request correlation id. */
  traceId: string;
}
