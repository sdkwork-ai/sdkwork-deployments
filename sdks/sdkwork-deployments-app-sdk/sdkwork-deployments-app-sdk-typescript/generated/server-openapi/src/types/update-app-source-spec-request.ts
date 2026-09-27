import type { AppClientArchitecture } from './app-client-architecture';
import type { AppSourceSpecRoute } from './app-source-spec-route';
import type { AppSourceSpecStatus } from './app-source-spec-status';
import type { SdkworkRuntimeTarget } from './sdkwork-runtime-target';

/** Partial update. `specKey` is intentionally absent: it is the projected variant key, and renaming it would silently re-point every CLIENT_CLASS rule that names it. */
export interface UpdateAppSourceSpecRequest {
  label?: string;
  runtimeTarget?: SdkworkRuntimeTarget;
  clientArchitecture?: AppClientArchitecture;
  /** Whole-set replacement of this spec's routes. Not additive: a class left out loses its routing rule, which is the only way "stop serving phones from this spec" is expressible. */
  clientClassRoutes?: AppSourceSpecRoute[];
  pathPrefix?: string;
  handler?: 'STATIC' | 'SPA' | 'WIKI';
  indexFiles?: string[];
  spaFallback?: string | null;
  isDefault?: boolean;
  priority?: number;
  status?: AppSourceSpecStatus;
}
