import type { CapabilityEmbedAuthMode } from './capability-embed-auth-mode';
import type { CapabilityEmbedKind } from './capability-embed-kind';
import type { CapabilityEmbedTransport } from './capability-embed-transport';
import type { CapabilityRowStatus } from './capability-row-status';

export interface EmbedDefinition {
  key: string;
  kind: CapabilityEmbedKind;
  serverRef?: string;
  endpointUrl?: string;
  transport?: CapabilityEmbedTransport;
  authMode?: CapabilityEmbedAuthMode;
  scopes?: string[];
  sortOrder?: number;
  status?: CapabilityRowStatus;
}
