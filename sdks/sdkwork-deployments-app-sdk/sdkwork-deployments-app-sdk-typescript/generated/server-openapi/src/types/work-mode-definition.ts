import type { CapabilityRowStatus } from './capability-row-status';
import type { LocalizedText } from './localized-text';
import type { WorkModeTool } from './work-mode-tool';

export interface WorkModeDefinition {
  key: string;
  name?: LocalizedText;
  description?: LocalizedText;
  systemPrompt: string;
  iconRef?: string;
  sortOrder?: number;
  isDefault?: boolean;
  status?: CapabilityRowStatus;
  tools?: WorkModeTool[];
}
