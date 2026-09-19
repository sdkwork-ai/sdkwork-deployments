import type { AppBindingDefinition } from './app-binding-definition';
import type { AppDeliveryPolicy } from './app-delivery-policy';
import type { AppMountDefinition } from './app-mount-definition';
import type { AppObservabilityPolicy } from './app-observability-policy';
import type { AppPublishEnvironment } from './app-publish-environment';
import type { AppResourceDefinition } from './app-resource-definition';
import type { AppRuntimeLimits } from './app-runtime-limits';
import type { AppSecurityPolicy } from './app-security-policy';
import type { AppVariantDefinition } from './app-variant-definition';
import type { AppVariantRuleDefinition } from './app-variant-rule-definition';
import type { CompositionKey } from './composition-key';

export interface UpdateAppCompositionRequest {
  environment: AppPublishEnvironment;
  defaultVariantKey: CompositionKey;
  resources: AppResourceDefinition[];
  variants: AppVariantDefinition[];
  variantRules?: AppVariantRuleDefinition[];
  mounts: AppMountDefinition[];
  bindings: AppBindingDefinition[];
  deliveryPolicy?: AppDeliveryPolicy;
  securityPolicy?: AppSecurityPolicy;
  limits?: AppRuntimeLimits;
  observabilityPolicy?: AppObservabilityPolicy;
}
