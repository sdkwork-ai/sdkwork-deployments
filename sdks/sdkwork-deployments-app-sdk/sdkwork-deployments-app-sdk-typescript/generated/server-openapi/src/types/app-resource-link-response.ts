import type { AppPublishEnvironment } from './app-publish-environment';
import type { AppResourceKind } from './app-resource-kind';
import type { AppResourceLinkStatus } from './app-resource-link-status';

export interface AppResourceLinkResponse {
  id: string;
  appId: string;
  environment: AppPublishEnvironment;
  resourceKind: AppResourceKind;
  /** The provider-side identifier: the WeChat AppID for an official account or mini program, the merchant account number for a payment account. */
  resourceRef: string;
  providerCode?: string;
  /** The environment's default account *for its provider*. Always false for the single-instance kinds, which have no default to elect. */
  isDefault: boolean;
  bindingStatus: AppResourceLinkStatus;
  /** Display snapshot and provider facts the binding was approved with. */
  metadata: Record<string, unknown>;
  createdAt: string;
  updatedAt: string;
  version: string;
}
