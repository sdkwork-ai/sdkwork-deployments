import type { AppKind } from './app-kind';
import type { AppOwnerType } from './app-owner-type';
import type { AppStatus } from './app-status';

export interface AppResponse {
  id: string;
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
  /** The owning user, present only for `ownerType: USER`. An id and not a display name: this service owns no user table, and a stored name drifts on rename. */
  ownerUserId?: string;
  /** The resolved owner subject — `user_id` for `USER`, `organization_id` for `ORGANIZATION`, absent for the two tenant-wide levels whose scope is the tenant itself. */
  ownerId?: string;
  /** Tenant scope. Exposed so a platform operator can locate an app; a tenant member only ever sees their own. */
  tenantId: string;
  /** Organization scope. The column stores `0` for "none", which is reported as absent rather than as an organization numbered zero. */
  organizationId?: string;
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
