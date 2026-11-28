import type { CapabilityRootTexts } from './capability-root-texts';
import type { DistributionScope } from './distribution-scope';
import type { EmbedDefinition } from './embed-definition';
import type { MarketAssetDefinition } from './market-asset-definition';
import type { ModelBindingDefinition } from './model-binding-definition';
import type { SceneDefinition } from './scene-definition';
import type { WorkModeDefinition } from './work-mode-definition';

export interface ReplaceAppCapabilityRequest {
  distributionScope?: DistributionScope;
  avatarUrl?: string;
  oauthClientId?: string;
  defaultWorkModeKey?: string;
  texts?: CapabilityRootTexts;
  workModes?: WorkModeDefinition[];
  scenes?: SceneDefinition[];
  models?: ModelBindingDefinition[];
  market?: MarketAssetDefinition[];
  embeds?: EmbedDefinition[];
  expectedVersion?: string;
  submitForReview?: boolean;
}
