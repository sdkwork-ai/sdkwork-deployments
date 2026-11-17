import type { AppKind } from './app-kind';
import type { AppOwnerType } from './app-owner-type';
import type { AppStatus } from './app-status';

export interface AppResponse {
  /** Opaque, stable public address of the application (`deploy_app.uuid`). Distinct from `deployApplicationId`, which is the decimal id IAM anchors on. */
  id: string;
  /** The application's decimal primary key (`deploy_app.id`). */
  deployApplicationId: string;
  name: string;
  slug: string;
  appKind: AppKind;
  appStatus: AppStatus;
  description?: string;
  /** Legacy web publishing type (1..6). Reserved; use appKind. */
  type?: number;
  runtimeConfig?: Record<string, unknown>;
  currentRevisionId?: string;
  desiredRevisionId?: string;
  defaultEnvironment: string;
  metadata?: Record<string, unknown>;
  platformTargetCount?: string;
  appDomainLabel?: string;
  appDomainSuffixes?: string[];
  latestReleaseTag?: string;
  ownerType: AppOwnerType;
  /** The same ownership mode as `ownerType`, under its product name. */
  ownershipScope: AppOwnerType;
  ownerUserId?: string;
  ownerId?: string;
  tenantId: string;
  /** "0" is the tenant-level encoding (the TENANT mode), a positive value is the ORGANIZATION mode's organization, and USER rows report "0" with `ownerUserId` present. */
  organizationId: string;
  createdBy?: string;
  updatedBy?: string;
  activatedAt?: string;
  pausedAt?: string;
  archivedAt?: string;
  nginxConfigOverridden: boolean;
  createdAt: string;
  updatedAt: string;
  version: string;
}
