import type { CapabilityToolKind } from './capability-tool-kind';

export interface WorkModeTool {
  kind: CapabilityToolKind;
  ref: string;
  policy?: Record<string, unknown>;
  sortOrder?: number;
}
