import type { AppClientArchitecture } from './app-client-architecture';
import type { AppSourceSpecRoute } from './app-source-spec-route';
import type { CompositionKey } from './composition-key';
import type { SdkworkRuntimeTarget } from './sdkwork-runtime-target';

export interface AppSourceSpecDefinition {
  specKey: CompositionKey;
  label: string;
  runtimeTarget?: SdkworkRuntimeTarget;
  clientArchitecture?: AppClientArchitecture;
  /** Which client classes this spec serves and at which preference. A class's default is its lowest-ranked spec, and each rank within a class belongs to one spec only, so "the default spec for MOBILE" is always defined. Ranks may be sparse while a set is being built up. */
  clientClassRoutes?: AppSourceSpecRoute[];
  pathPrefix?: string;
  handler?: 'STATIC' | 'SPA' | 'WIKI';
  indexFiles?: string[];
  spaFallback?: string;
  isDefault?: boolean;
  priority?: number;
}
