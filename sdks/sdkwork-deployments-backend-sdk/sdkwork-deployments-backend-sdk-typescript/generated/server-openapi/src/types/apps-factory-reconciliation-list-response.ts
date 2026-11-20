import type { AppFactoryReconciliation } from './app-factory-reconciliation';

export interface AppsFactoryReconciliationListResponse {
  code: 0;
  data: unknown & { item: AppFactoryReconciliation; };
  /** Server-owned request correlation id. */
  traceId: string;
}
