import type { AppPublishEnvironment } from './app-publish-environment';
import type { AppResourceKind } from './app-resource-kind';
import type { AppResourceLinkStatus } from './app-resource-link-status';

export interface UpsertAppResourceLinkRequest {
  environment: AppPublishEnvironment;
  resourceKind: AppResourceKind;
  resourceRef: string;
  /** Required for `payment_account`. The default account is unique per provider, and a NULL provider would make every row its own default. */
  providerCode?: string;
  /** Only meaningful for a payment account; refused for the single-instance kinds. */
  isDefault?: boolean;
  bindingStatus?: AppResourceLinkStatus;
  /** Display snapshot and provider facts to store with the binding. */
  metadata?: Record<string, unknown>;
}
