/**
 * Application upload declaration constants.
 *
 * Authority: `DRIVE_SPEC.md` section 18 (Application Upload Declaration Contract).
 * Declared values live in `apps/sdkwork-deployments-pc/specs/upload.declaration.json`;
 * this module carries them into code so upload call sites reference a constant instead of
 * repeating literals. Call sites MUST NOT inline these values, and the declaration MUST NOT
 * be duplicated as a second local authority.
 *
 * `tests/upload-declaration.test.ts` asserts this module and the declaration file agree, so a
 * change to one without the other fails the gate rather than drifting silently.
 */

/** One declared upload purpose. Mirrors a `declarations[]` entry in the declaration file. */
export interface UploadDeclarationEntry {
  readonly appResourceIdKind: "application" | "entity" | "draft";
  readonly appResourceType: string;
  readonly purpose: string;
  readonly retention: "long_term" | "temporary";
  readonly scene: string;
  readonly source: string;
  readonly uploadProfileCode: string;
}

/** This application's canonical `appId` from `sdkwork.app.config.json`. */
export const DEPLOY_APP_ID = "sdkwork-deployments-pc" as const;

/**
 * Deployment package upload: a built artifact (local archive, Git checkout, or a Drive-picked
 * archive) uploaded so a release can reference it.
 */
export const DEPLOY_ARTIFACT_UPLOAD = {
  appResourceIdKind: "application",
  appResourceType: "deploy.artifact",
  purpose:
    "Application deployment package (local archive, Git checkout, or Drive-picked archive) uploaded so a release can reference a built artifact.",
  retention: "long_term",
  scene: "deployment-package",
  source: "sdkwork-deployments-pc",
  uploadProfileCode: "archive",
} as const satisfies UploadDeclarationEntry;

/**
 * App-store preview media upload: icon, cover, and screenshots attached to an application after
 * the application record exists. The media kind is a closed `DeployAppKind`-driven dimension and
 * deliberately does not enter `scene`: one fixed scene keeps one upload origin from splitting into
 * one statistic row per app kind.
 */
export const DEPLOY_APP_MEDIA_UPLOAD = {
  appResourceIdKind: "application",
  appResourceType: "deploy.app.media",
  purpose:
    "App-store preview media (icon, cover, screenshots) attached to an application after the application record exists.",
  retention: "long_term",
  scene: "deploy-app-media",
  source: "sdkwork-deployments-pc",
  uploadProfileCode: "image",
} as const satisfies UploadDeclarationEntry;

/** Every declared upload purpose for this application. */
export const DEPLOY_UPLOAD_DECLARATIONS: readonly UploadDeclarationEntry[] = [
  DEPLOY_ARTIFACT_UPLOAD,
  DEPLOY_APP_MEDIA_UPLOAD,
];
