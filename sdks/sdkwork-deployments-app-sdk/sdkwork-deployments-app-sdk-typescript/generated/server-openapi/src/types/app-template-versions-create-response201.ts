import type { AppTemplateVersionResponse } from './app-template-version-response';

export interface AppTemplateVersionsCreateResponse201 {
  code: 0;
  data: unknown & { item: AppTemplateVersionResponse; };
  /** Server-owned request correlation id. */
  traceId: string;
}
