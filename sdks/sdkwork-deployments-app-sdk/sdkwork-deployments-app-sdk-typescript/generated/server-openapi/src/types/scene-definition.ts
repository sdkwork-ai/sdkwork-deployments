import type { CapabilityRowStatus } from './capability-row-status';
import type { LocalizedText } from './localized-text';
import type { SceneAsset } from './scene-asset';

export interface SceneDefinition {
  key: string;
  title?: LocalizedText;
  subtitle?: LocalizedText;
  promptTemplate: string;
  iconRef?: string;
  sortOrder?: number;
  isFeatured?: boolean;
  status?: CapabilityRowStatus;
  assets?: SceneAsset[];
}
