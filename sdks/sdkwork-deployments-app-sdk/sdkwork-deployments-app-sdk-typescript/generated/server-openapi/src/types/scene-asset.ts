import type { CapabilityAssetKind } from './capability-asset-kind';

export interface SceneAsset {
  kind: CapabilityAssetKind;
  ref: string;
  snapshot?: Record<string, unknown>;
  sortOrder?: number;
}
