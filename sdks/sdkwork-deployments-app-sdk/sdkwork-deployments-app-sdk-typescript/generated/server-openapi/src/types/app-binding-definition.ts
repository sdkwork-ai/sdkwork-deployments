import type { CompositionKey } from './composition-key';
import type { RedirectBindingAction } from './redirect-binding-action';
import type { ServeBindingAction } from './serve-binding-action';

export interface AppBindingDefinition {
  key: CompositionKey;
  domainId: string;
  pathPrefix?: string;
  action: ServeBindingAction | RedirectBindingAction;
}
