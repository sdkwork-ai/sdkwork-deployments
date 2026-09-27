import type { AppBindingDefinition } from './app-binding-definition';
import type { AppDeliveryPolicy } from './app-delivery-policy';
import type { AppMountDefinition } from './app-mount-definition';
import type { AppObservabilityPolicy } from './app-observability-policy';
import type { AppPublishEnvironment } from './app-publish-environment';
import type { AppResourceDefinition } from './app-resource-definition';
import type { AppRuntimeLimits } from './app-runtime-limits';
import type { AppSecurityPolicy } from './app-security-policy';
import type { AppSourceSpecDefinition } from './app-source-spec-definition';
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
  /** Replace this environment's source specs in the same transaction. Absent means "leave the spec set alone", which keeps a composition update from wiping a spec authored through the source-spec endpoints; present means "make the set exactly this". Declared specs are projected into Variants, CLIENT_CLASS VariantRules, Mounts and Resources for the revision this call compiles, and pre-existing specs are carried by the descriptor even though the request never mentioned them. */
  sourceSpecs?: AppSourceSpecDefinition[];
  deliveryPolicy?: AppDeliveryPolicy;
  securityPolicy?: AppSecurityPolicy;
  limits?: AppRuntimeLimits;
  observabilityPolicy?: AppObservabilityPolicy;
}
