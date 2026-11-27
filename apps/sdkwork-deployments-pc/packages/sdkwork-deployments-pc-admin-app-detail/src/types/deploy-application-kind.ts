/**
 * Endpoint-kind vocabulary of a deployment application.
 *
 * A deployment application is one endpoint (`AppKind` on the deploy API): the
 * web surfaces, the two mini-program providers, the native mobile kinds,
 * HarmonyOS, desktop, and API services are separate applications rather than
 * variants of one. `SPA_WEB` and `STATIC_WEB` are the legacy rendering-model
 * kinds kept for existing rows; an unrecognized value degrades to `UNKNOWN`.
 */
export const DEPLOY_APPLICATION_KINDS = [
  "H5_WEB",
  "PC_WEB",
  "WECHAT_MINIPROGRAM",
  "DOUYIN_MINIPROGRAM",
  "FLUTTER_APP",
  "IOS_APP",
  "ANDROID_APP",
  "HARMONYOS_APP",
  "API_SERVICE",
  "DESKTOP_APP",
  "STATIC_WEB",
  "SPA_WEB",
] as const;

export type DeployApplicationKind = (typeof DEPLOY_APPLICATION_KINDS)[number];

/** Lifecycle status of a deployment application (`AppStatus` on the wire). */
export const DEPLOY_APPLICATION_STATUSES = [
  "DRAFT",
  "READY",
  "ACTIVE",
  "PAUSED",
  "ARCHIVED",
  "FAILED",
] as const;

export type DeployApplicationStatus = (typeof DEPLOY_APPLICATION_STATUSES)[number];
