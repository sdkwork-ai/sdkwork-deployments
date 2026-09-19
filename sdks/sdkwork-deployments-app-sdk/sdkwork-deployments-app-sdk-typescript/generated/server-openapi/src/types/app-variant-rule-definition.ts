import type { ClientClassVariantMatch } from './client-class-variant-match';
import type { CompositionKey } from './composition-key';
import type { PathPrefixVariantMatch } from './path-prefix-variant-match';

export interface AppVariantRuleDefinition {
  key: CompositionKey;
  targetVariantKey: CompositionKey;
  priority: number;
  match: PathPrefixVariantMatch | ClientClassVariantMatch;
}
