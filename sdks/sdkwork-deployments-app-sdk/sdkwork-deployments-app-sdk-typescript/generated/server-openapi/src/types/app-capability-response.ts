import type { CapabilityRootTexts } from './capability-root-texts';
import type { CapabilityStatus } from './capability-status';
import type { DistributionScope } from './distribution-scope';
import type { EmbedDefinition } from './embed-definition';
import type { MarketAssetDefinition } from './market-asset-definition';
import type { ModelBindingDefinition } from './model-binding-definition';
import type { SceneDefinition } from './scene-definition';
import type { WorkModeDefinition } from './work-mode-definition';

export interface AppCapabilityResponse {
  appId: string;
  status: CapabilityStatus;
  distributionScope: DistributionScope;
  avatarUrl?: string;
  /** The IAM OAuth client (`client_id`) that serves this application. A string reference, not a foreign key: the client lives in `iam_application_installation_credential`, whose lifecycle this module does not own. */
  oauthClientId?: string;
  defaultWorkModeKey?: string;
  configSchemaVersion: number;
  currentRevisionId?: string;
  publishedRevisionId?: string;
  publishedAt?: string;
  texts?: CapabilityRootTexts;
  workModes?: WorkModeDefinition[];
  scenes?: SceneDefinition[];
  models?: ModelBindingDefinition[];
  market?: MarketAssetDefinition[];
  embeds?: EmbedDefinition[];
  createdAt?: string;
  updatedAt?: string;
  version: string;
}
