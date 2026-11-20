import type { AppUninstallResponse } from './app-uninstall-response';

export interface AppsUninstallResponse {
  code: 0;
  data: unknown & { item: AppUninstallResponse; };
  /** Server-owned request correlation id. */
  traceId: string;
}
