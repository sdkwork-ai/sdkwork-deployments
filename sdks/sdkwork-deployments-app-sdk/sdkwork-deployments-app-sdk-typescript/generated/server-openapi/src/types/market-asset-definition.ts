import type { CapabilityAssetKind } from './capability-asset-kind';
import type { CapabilityMarketSection } from './capability-market-section';
import type { CapabilityRowStatus } from './capability-row-status';

export interface MarketAssetDefinition {
  section: CapabilityMarketSection;
  kind: CapabilityAssetKind;
  ref: string;
  categoryCode?: string;
  sortOrder?: number;
  isFeatured?: boolean;
  status?: CapabilityRowStatus;
  snapshot?: Record<string, unknown>;
}
