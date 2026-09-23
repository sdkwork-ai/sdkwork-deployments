import type { AppOwnerType } from './app-owner-type';
import type { AppStatus } from './app-status';

export interface UpdateAppRequest {
  name?: string;
  description?: string;
  /** Move the app to another ownership level. Absent leaves it alone. Changing the level rewrites the owner pointer in the same statement, because the schema constrains the two to agree: `USER` takes `ownerUserId`, `ORGANIZATION` uses the app's organization, and `TENANT` clears the user owner. `PLATFORM` is rejected for the same reason `apps.create` rejects it. */
  ownerType?: AppOwnerType;
  /** The user that should own the app when moving it to `USER`. Absent keeps the current owner; ignored at every other level. */
  ownerUserId?: string;
  appStatus?: AppStatus;
  defaultEnvironment?: string;
  /** Free-form JSONB merged into deploy_app.metadata. */
  metadata?: Record<string, unknown>;
  /** The `<appId>` prefix of the app's default publishing hostnames. Must be one lowercase DNS label (alphanumeric plus interior hyphens). `null` clears the override so the slug is used again. */
  appDomainLabel?: string | null;
  /** Per-app override of the platform app-domain suffix catalog. Each entry is a lowercase dotted domain without a leading dot. `null` clears the override so the platform catalog applies. */
  appDomainSuffixes?: string[] | null;
}
