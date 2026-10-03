// Unit tests for the shared Drive image-upload service binding for deploy-app
// media (`createDeployAppMediaImageService`): the whole point of the binding is
// that media enters Drive through the DECLARED image profile
// (`DEPLOY_APP_MEDIA_UPLOAD`, `DRIVE_SPEC.md` §18) — the regression this pins
// is the earlier hand-rolled `uploader.uploadArchive` call, which sent icon /
// cover / screenshot bytes through the wrong upload profile.
import { describe, expect, it, vi } from "vitest";
import { createDeployAppMediaImageService } from "@sdkwork/deployments-pc-commons";

/** Uploader spy that answers one successful image upload. */
function spyUploader() {
  const requests: unknown[] = [];
  const uploadImage = vi.fn(async (request: unknown) => {
    requests.push(request);
    return {
      uploadSession: { id: "session-1" },
      uploadItem: {
        id: "item-1",
        spaceId: "space-1",
        nodeId: "node-1",
        contentType: "image/png",
        contentLength: "4",
        originalFileName: "icon.png",
      },
    };
  });
  return {
    requests,
    uploadImage,
    uploadAvatar: vi.fn(),
    uploadThumbnail: vi.fn(),
  };
}

/** Same-origin Drive nodes content API stub (never exercised by upload). */
const nodes = {
  content: {
    retrieve: vi.fn(async () => ({
      nodeId: "node-1",
      encoding: "base64" as const,
      content: "",
      sizeBytes: "0",
      returnedRangeStart: "0",
      returnedRangeLength: "0",
      hasMore: false,
    })),
  },
};

const pngFile = () => new File([new Uint8Array([0x89, 0x50, 0x4e, 0x47])], "icon.png", { type: "image/png" });

describe("createDeployAppMediaImageService", () => {
  it("uploads through the declared image profile — never the archive uploader", async () => {
    const uploader = spyUploader();
    const service = createDeployAppMediaImageService({
      uploader: uploader as never,
      nodes: nodes as never,
    });

    const value = await service.upload({ file: pngFile(), appResourceId: "app-1" });

    expect(uploader.uploadImage).toHaveBeenCalledTimes(1);
    expect(uploader.uploadAvatar).not.toHaveBeenCalled();
    expect(uploader.uploadThumbnail).not.toHaveBeenCalled();
    expect(uploader.requests[0]).toMatchObject({
      appResourceType: "deploy.app.media",
      appResourceId: "app-1",
      scene: "deploy-app-media",
      source: "sdkwork-deployments-pc",
      uploadProfileCode: "image",
      retention: { mode: "long_term" },
      originalFileName: "icon.png",
      contentType: "image/png",
    });
    // The persisted value carries only the stable Drive reference.
    expect(value.uri).toBe("drive://spaces/space-1/nodes/node-1");
    expect(value.metadata?.drive).toMatchObject({ spaceId: "space-1", nodeId: "node-1" });
  });

  it("refuses to upload before the application record exists (persist-first)", async () => {
    const uploader = spyUploader();
    const service = createDeployAppMediaImageService({
      uploader: uploader as never,
      nodes: nodes as never,
    });

    await expect(service.upload({ file: pngFile(), appResourceId: "" })).rejects.toMatchObject({
      code: "missing-app-resource-id",
    });
    expect(uploader.uploadImage).not.toHaveBeenCalled();
  });
});
