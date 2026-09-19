import type { AppStatus } from './app-status';

export interface UpdateAppRequest {
  name?: string;
  description?: string;
  appStatus?: AppStatus;
  defaultEnvironment?: string;
  /** Free-form JSONB merged into deploy_app.metadata. */
  metadata?: Record<string, unknown>;
  /** The `<appId>` prefix of the app's default publishing hostnames. Must be one lowercase DNS label (alphanumeric plus interior hyphens). `null` clears the override so the slug is used again. */
  appDomainLabel?: string | null;
  /** Per-app override of the platform app-domain suffix catalog. Each entry is a lowercase dotted domain without a leading dot. `null` clears the override so the platform catalog applies. */
  appDomainSuffixes?: string[] | null;
}
