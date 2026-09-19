import type { AppKind } from './app-kind';
import type { AppStatus } from './app-status';

export interface AppResponse {
  id: string;
  name: string;
  slug: string;
  appKind: AppKind;
  appStatus: AppStatus;
  description?: string;
  siteId?: string;
  defaultEnvironment: string;
  /** Echo of deploy_app.metadata. */
  metadata?: Record<string, unknown>;
  platformTargetCount?: string;
  /** The effective `<appId>` prefix used in the app's default publishing hostnames (`<appDomainLabel>.app[-<env>].<suffix>`): the explicit override when set, otherwise the slug. */
  appDomainLabel?: string;
  /** The effective app-domain suffix catalog the app publishes on: the per-app override when set, otherwise the platform catalog. */
  appDomainSuffixes?: string[];
  latestReleaseTag?: string;
  createdAt: string;
  updatedAt: string;
  version: string;
}
