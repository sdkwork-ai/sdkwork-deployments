import type { AppKind } from './app-kind';

export interface CreateAppRequest {
  name: string;
  slug?: string;
  appKind: AppKind;
  description?: string;
  siteId?: string;
  defaultEnvironment?: string;
  /** Free-form JSONB persisted into deploy_app.metadata (category, media, version, releaseNotes). */
  metadata?: Record<string, unknown>;
  /** The `<appId>` prefix of the app's default publishing hostnames (`<appDomainLabel>.app[-<env>].<suffix>`). Must be one lowercase DNS label (alphanumeric plus interior hyphens); absent means the slug is used. */
  appDomainLabel?: string;
  /** Per-app override of the platform app-domain suffix catalog. Each entry is a lowercase dotted domain without a leading dot; absent means the platform catalog applies. */
  appDomainSuffixes?: string[];
  idempotencyKey?: string;
}
