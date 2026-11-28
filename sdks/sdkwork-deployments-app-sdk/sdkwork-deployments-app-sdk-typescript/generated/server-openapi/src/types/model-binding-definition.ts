import type { CapabilityRowStatus } from './capability-row-status';

export interface ModelBindingDefinition {
  providerCode: string;
  modelKey: string;
  sortOrder?: number;
  isDefault?: boolean;
  status?: CapabilityRowStatus;
}
