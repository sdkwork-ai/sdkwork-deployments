/**
 * Shared Drive image-upload service for deploy-app store media (icon / cover /
 * screenshots).
 *
 * Binds this application's declared upload intent (`DEPLOY_APP_MEDIA_UPLOAD`,
 * `DRIVE_SPEC.md` section 18) onto the platform's reusable image-upload core so
 * media enters Drive through the declared image profile instead of hand-rolled
 * uploader calls. The factory lives next to the declaration constant because
 * the declaration is the only application-specific input — the Drive client
 * slices arrive structurally from the caller (console-core owns the SDK
 * dependency; commons must not depend back on it).
 */
import {
  createDriveNodesImagePreviewReader,
  createDriveUploadImageService,
  type DriveImagePreviewReaderLike,
  type DriveUploadImageService,
  type DriveUploadImageUploaderLike,
} from "@sdkwork/drive-upload-image-core";
import { DEPLOY_APP_MEDIA_UPLOAD } from "./uploadDeclaration.ts";

/** Structural slice of the generated Drive nodes API the preview reader binds. */
export type DeployAppMediaNodesSource = Parameters<typeof createDriveNodesImagePreviewReader>[0];

/** Drive client slices the factory needs — both satisfy the shared core's contracts. */
export interface DeployAppMediaDriveSource {
  readonly uploader: DriveUploadImageUploaderLike;
  readonly nodes: DeployAppMediaNodesSource;
}

/**
 * Builds the declared image-upload service for deploy-app media.
 *
 * The preview reader stays same-origin and size-capped over
 * `drive.nodes.content.retrieve` (`DRIVE_SPEC.md` section 9); retention travels
 * from the declaration (`long_term`). Call this once per service construction —
 * creating it is pure, so re-creating per dialog mount is fine.
 */
export function createDeployAppMediaImageService(drive: DeployAppMediaDriveSource): DriveUploadImageService {
  const previewReader: DriveImagePreviewReaderLike = createDriveNodesImagePreviewReader(drive.nodes);
  return createDriveUploadImageService({
    uploader: drive.uploader,
    declaration: DEPLOY_APP_MEDIA_UPLOAD,
    previewReader,
  });
}
