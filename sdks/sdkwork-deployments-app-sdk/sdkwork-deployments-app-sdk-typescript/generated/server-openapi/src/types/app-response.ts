import type { AppKind } from './app-kind';
import type { AppOwnerType } from './app-owner-type';
import type { AppStatus } from './app-status';

export interface AppResponse {
  /** Opaque, stable public address of the application (`deploy_app.uuid`). Every app endpoint resolves against this value; it is **not** the decimal id IAM anchors on — that is `deployApplicationId`. */
  id: string;
  /** The application's **decimal** primary key (`deploy_app.id`), and the **IAM installation anchor**: `apps.identityProvision` sends exactly this value to IAM as `deployApplicationId`, IAM stores it in `iam_tenant_application.deploy_application_id` under a unique index, and the installation's runtime `app_id` / token `aud` are this same decimal number. Read it from here after `apps.create` instead of deriving or guessing it. Distinct from `id`, which is the uuid. */
  deployApplicationId: string;
  name: string;
  slug: string;
  appKind: AppKind;
  appStatus: AppStatus;
  description?: string;
  /** Legacy web publishing type (1..6, `deploy_app.type`). Reserved: the column predates the unified app model and no consumer reads it. It is declared here because the server has always serialized it, so the schema previously contradicted its own `additionalProperties: false`. Use `appKind` to describe what an app is; do not grow this field. */
  type?: number;
  /** Echo of `deploy_app.runtime_config`. Declared for the same reason as `type` — serialized by the server, previously absent from the schema. */
  runtimeConfig?: Record<string, unknown>;
  /** The revision last observed converged on a web node. Differs from `desiredRevisionId` exactly when a release has not taken effect yet. */
  currentRevisionId?: string;
  /** The revision the last publish wrote. `desired != current` is an un-applied release. */
  desiredRevisionId?: string;
  defaultEnvironment: string;
  /** Echo of deploy_app.metadata. */
  metadata?: Record<string, unknown>;
  platformTargetCount?: string;
  /** The effective `<appId>` prefix used in the app's default publishing hostnames (`<appDomainLabel>.app[-<env>].<suffix>`): the explicit override when set, otherwise the slug. */
  appDomainLabel?: string;
  /** The effective app-domain suffix catalog the app publishes on: the per-app override when set, otherwise the platform catalog. */
  appDomainSuffixes?: string[];
  latestReleaseTag?: string;
  ownerType: AppOwnerType;
  /** The ownership mode under its product name — `TENANT` (`organizationId` `"0"`), `ORGANIZATION` (`organizationId` > `0`), or `USER` (`ownerUserId` present). The same fact as `ownerType`, which is kept for compatibility; the two are always equal and are not a second stored column. */
  ownershipScope: AppOwnerType;
  /** The owning user, present only for `ownerType: USER`. An id and not a display name: this service owns no user table, and a stored name drifts on rename. */
  ownerUserId?: string;
  /** The resolved owner subject — `user_id` for `USER`, `organization_id` for `ORGANIZATION`, absent for the two tenant-wide levels whose scope is the tenant itself. */
  ownerId?: string;
  /** Tenant scope. Exposed so a platform operator can locate an app; a tenant member only ever sees their own. */
  tenantId: string;
  /** Organization scope. `"0"` is the tenant-level encoding (the `TENANT` mode), a positive value is the organization the `ORGANIZATION` mode belongs to, and `USER` rows report `"0"` with `ownerUserId` present. Always returned, so the three modes are distinguishable without a second lookup. */
  organizationId: string;
  createdBy?: string;
  updatedBy?: string;
  /** When the app last entered `ACTIVE`. */
  activatedAt?: string;
  /** When the app last entered `PAUSED`. */
  pausedAt?: string;
  /** When the app was archived (the retirement transition). */
  archivedAt?: string;
  /** Whether an operator has overridden the generated app-level nginx-compatible configuration (`nginx_conf_sha256 IS NOT NULL`). */
  nginxConfigOverridden: boolean;
  createdAt: string;
  updatedAt: string;
  version: string;
}
