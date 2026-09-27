import type { AppKind } from './app-kind';
import type { AppOwnerType } from './app-owner-type';
import type { AppSourceSpecDefinition } from './app-source-spec-definition';

export interface CreateAppRequest {
  name: string;
  slug?: string;
  appKind: AppKind;
  /** Ownership level for the new application. Absent derives it from the request principal — `USER` when a user subject is present, which is the normal case, otherwise `TENANT`. `PLATFORM` is rejected: no permission in this surface distinguishes a platform operator from any other tenant member, so honouring it would let a member publish an app visible to every tenant. */
  ownerType?: AppOwnerType;
  description?: string;
  defaultEnvironment?: string;
  /** Free-form JSONB persisted into deploy_app.metadata (category, media, version, releaseNotes). */
  metadata?: Record<string, unknown>;
  /** The `<appId>` prefix of the app's default publishing hostnames (`<appDomainLabel>.app[-<env>].<suffix>`). Must be one lowercase DNS label (alphanumeric plus interior hyphens); absent means the slug is used. */
  appDomainLabel?: string;
  /** Per-app override of the platform app-domain suffix catalog. Each entry is a lowercase dotted domain without a leading dot; absent means the platform catalog applies. */
  appDomainSuffixes?: string[];
  /** Optional source specs declared with the app. One app legitimately owns several sources of different specs (a PC build and an H5 build), so declaring them here lets a single create call produce an app whose one publishing hostname already routes by client class. They land in the app's default environment. Exactly one may set `isDefault` when the array is non-empty: it is the fallback for clients no rule classifies. */
  sourceSpecs?: AppSourceSpecDefinition[];
  idempotencyKey?: string;
}
