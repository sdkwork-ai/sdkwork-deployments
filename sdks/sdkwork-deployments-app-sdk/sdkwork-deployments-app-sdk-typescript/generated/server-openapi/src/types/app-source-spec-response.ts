import type { AppClientArchitecture } from './app-client-architecture';
import type { AppSourceBindingResponse } from './app-source-binding-response';
import type { AppSourceBindingStatus } from './app-source-binding-status';
import type { AppSourceSpecRoute } from './app-source-spec-route';
import type { AppSourceSpecStatus } from './app-source-spec-status';
import type { CompositionKey } from './composition-key';
import type { SdkworkRuntimeTarget } from './sdkwork-runtime-target';

export interface AppSourceSpecResponse {
  id: string;
  specKey: CompositionKey;
  label: string;
  runtimeTarget: SdkworkRuntimeTarget;
  clientArchitecture: AppClientArchitecture;
  /** The client classes this spec serves, each with its preference rank. Each becomes one CLIENT_CLASS routing rule whose priority is the rank, so one source can serve several classes (an H5 bundle serves MOBILE and TABLET) and a class can name fallbacks after its default. */
  clientClassRoutes: AppSourceSpecRoute[];
  pathPrefix: string;
  handler: 'STATIC' | 'SPA' | 'WIKI';
  indexFiles: string[];
  spaFallback?: string;
  /** App-level default variant, used for a client that matches no class route at all; at most one active per (app, environment). Per-client-class defaults are the `preference: 0` entries of `clientClassRoutes`. */
  isDefault: boolean;
  priority: number;
  status: AppSourceSpecStatus;
  sourceStatus: AppSourceBindingStatus;
  source?: AppSourceBindingResponse;
  createdAt: string;
  updatedAt: string;
  version: string;
}
