import type { FactoryReadinessCheck } from './factory-readiness-check';

export interface AppFactoryReadiness {
  status: 'READY' | 'PENDING' | 'BLOCKED';
  blockingStep?: string;
  checks: FactoryReadinessCheck[];
}
