import type { FactoryDanglingReference } from './factory-dangling-reference';

export interface AppFactoryReconciliation {
  /** `CLEAN` when no dangling reference was found, `DRIFT` when at least one was, `PARTIAL` when every check found nothing but some authority could not be inspected. */
  status: 'CLEAN' | 'DRIFT' | 'PARTIAL';
  appsChecked: string;
  dangling: FactoryDanglingReference[];
  unavailable: string[];
}
