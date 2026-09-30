import { describe, expect, it } from "vitest";

import type { CreateDownloadUrlResponse } from "@sdkwork/deployments-pc-console-core/sdk";
import {
  APP_AVATAR_GRADIENT_COUNT,
  appAvatarGradientClass,
  appAvatarIconKey,
  createAppIconUrlResolver,
  hashAppAvatarSeed,
  readAppIconDriveNodeId,
  type AppIconDriveClient,
} from "../packages/sdkwork-deployments-pc-console-publishing/src/service/app-icon.ts";

// The avatar tile is the row's first-read identity, so its two halves each get
// a unit seam: the *deterministic* half (palette + glyph) must never depend on
// anything but row data, and the *resolved* half (signed URL exchange) must
// degrade to "no icon" on every failure mode instead of turning a list render
// into an error surface.

const mediaIcon = (driveNodeId: string) => ({
  media: {
    icon: {
      driveNodeId,
      driveSpaceId: "space-1",
      uploadItemId: "item-1",
      uploadSessionId: "session-1",
      fileName: "icon.png",
      contentType: "image/png",
    },
  },
});

describe("app icon media reference parsing", () => {
  it("reads the drive node id out of the metadata the publish flow writes", () => {
    expect(readAppIconDriveNodeId(mediaIcon("node-42"))).toBe("node-42");
  });

  it("reads nothing when the app has no media group at all", () => {
    expect(readAppIconDriveNodeId(undefined)).toBeUndefined();
    expect(readAppIconDriveNodeId({})).toBeUndefined();
    expect(readAppIconDriveNodeId({ media: {} })).toBeUndefined();
  });

  it("reads nothing from a malformed or hostile media group", () => {
    // Older clients and hand-poked rows must degrade to the plain tile, never
    // throw from a render.
    expect(readAppIconDriveNodeId({ media: { icon: null } })).toBeUndefined();
    expect(readAppIconDriveNodeId({ media: { icon: "node-42" } })).toBeUndefined();
    expect(readAppIconDriveNodeId({ media: { icon: { driveNodeId: 42 } } })).toBeUndefined();
    expect(readAppIconDriveNodeId({ media: { icon: { driveNodeId: "" } } })).toBeUndefined();
    expect(readAppIconDriveNodeId("node-42")).toBeUndefined();
    expect(readAppIconDriveNodeId(42)).toBeUndefined();
  });
});

describe("app avatar glyph mapping", () => {
  it("maps every contract kind onto the publish dialog's glyph vocabulary", () => {
    const kinds = [
      "STATIC_WEB", "SPA_WEB", "API_SERVICE", "WECHAT_MINIPROGRAM", "DOUYIN_MINIPROGRAM",
      "IOS_APP", "ANDROID_APP", "HARMONYOS_APP", "DESKTOP_APP",
    ] as const;
    for (const appKind of kinds) {
      expect(appAvatarIconKey({ slug: "x", appKind })).toMatch(
        /^(static|pc|h5|api|mini-program|ios|android|harmony|desktop)$/,
      );
    }
  });

  it("follows the recorded web end for SPA_WEB, because the kind alone spans two glyphs", () => {
    expect(appAvatarIconKey({ slug: "x", appKind: "SPA_WEB", metadata: { surface: "h5" } })).toBe("h5");
    expect(appAvatarIconKey({ slug: "x", appKind: "SPA_WEB", metadata: { surface: "pc" } })).toBe("pc");
  });

  it("keeps the pc glyph when the surface was never recorded (unpublished app)", () => {
    expect(appAvatarIconKey({ slug: "x", appKind: "SPA_WEB" })).toBe("pc");
    expect(appAvatarIconKey({ slug: "x", appKind: "SPA_WEB", metadata: {} })).toBe("pc");
  });
});

describe("app avatar gradient", () => {
  it("is deterministic per slug and always inside the fixed palette", () => {
    for (const slug of ["webserver", "console-app", "drive", "iam", "a", "应用"]) {
      const first = appAvatarGradientClass(slug);
      expect(first).toBe(appAvatarGradientClass(slug));
      expect(first).toMatch(/^apps-avatar-gradient-\d+$/);
      const index = Number(first.slice("apps-avatar-gradient-".length));
      expect(index).toBeGreaterThanOrEqual(0);
      expect(index).toBeLessThan(APP_AVATAR_GRADIENT_COUNT);
      expect(APP_AVATAR_GRADIENT_COUNT).toBe(10);
    }
  });

  it("spreads distinct slugs across more than one bucket", () => {
    const buckets = new Set(
      ["alpha", "bravo", "charlie", "delta", "echo", "foxtrot", "golf", "hotel"].map(appAvatarGradientClass),
    );
    expect(buckets.size).toBeGreaterThan(1);
  });

  it("renders no class for an empty seed", () => {
    expect(appAvatarGradientClass("")).toBe("");
    expect(hashAppAvatarSeed("")).toBe(0x811c9dc5);
  });
});

interface ExchangeGate {
  readonly nodeId: string
  readonly options: { signal?: AbortSignal; timeout?: number } | undefined
  resolve: (response: CreateDownloadUrlResponse) => void
  reject: (cause: unknown) => void
}

/** A Drive download-urls API whose answers the test hands out one gate at a time. */
function createGatedDrive(): { calls: ExchangeGate[]; drive: AppIconDriveClient } {
  const calls: ExchangeGate[] = [];
  const drive: AppIconDriveClient = {
    nodes: {
      downloadUrls: {
        retrieve: (nodeId, _params, requestOptions) => {
          let resolve!: (response: CreateDownloadUrlResponse) => void;
          let reject!: (cause: unknown) => void;
          const promise = new Promise<CreateDownloadUrlResponse>((res, rej) => {
            resolve = res;
            reject = rej;
          });
          calls.push({ nodeId, options: requestOptions, resolve, reject });
          return promise;
        },
      },
    },
  };
  return { calls, drive };
}

const signed = (url: string, expiresAtEpochMs: string): CreateDownloadUrlResponse => ({
  downloadUrl: url,
  signedSourceUrl: url,
  expiresAtEpochMs,
  method: "GET",
});

/** Let queued continuations (semaphore wakeups, in-flight cleanup) run. */
const flush = () => new Promise<void>((resolveFlush) => { setTimeout(resolveFlush, 0); });

describe("app icon url resolver", () => {
  it("exchanges a node reference once, then serves the cached signed URL", async () => {
    const { calls, drive } = createGatedDrive();
    let clock = 1_000;
    const resolveIconUrl = createAppIconUrlResolver({ drive, now: () => clock });

    const first = resolveIconUrl("node-1", new AbortController().signal);
    expect(calls).toHaveLength(1);
    calls[0]!.resolve(signed("https://signed/one", String(clock + 300_000)));
    await expect(first).resolves.toBe("https://signed/one");

    clock += 10_000;
    await expect(resolveIconUrl("node-1", new AbortController().signal)).resolves.toBe("https://signed/one");
    expect(calls).toHaveLength(1);
  });

  it("deduplicates concurrent requests for the same node into one exchange", async () => {
    const { calls, drive } = createGatedDrive();
    const resolveIconUrl = createAppIconUrlResolver({ drive });

    const first = resolveIconUrl("node-1", new AbortController().signal);
    const second = resolveIconUrl("node-1", new AbortController().signal);
    expect(calls).toHaveLength(1);
    calls[0]!.resolve(signed("https://signed/one", String(Date.now() + 300_000)));
    await expect(first).resolves.toBe("https://signed/one");
    await expect(second).resolves.toBe("https://signed/one");
    expect(calls).toHaveLength(1);
  });

  it("retires a URL before its signature lapses (TTL safety margin)", async () => {
    const { calls, drive } = createGatedDrive();
    let clock = 1_000;
    const resolveIconUrl = createAppIconUrlResolver({ drive, now: () => clock });

    const first = resolveIconUrl("node-1", new AbortController().signal);
    // Signature lives 90s; the margin is 60s, so the URL is trusted only
    // before t+30s. One second past that boundary the cache must let go.
    calls[0]!.resolve(signed("https://signed/one", String(clock + 90_000)));
    await first;

    clock += 29_000;
    await resolveIconUrl("node-1", new AbortController().signal);
    expect(calls).toHaveLength(1);

    clock += 2_000;
    const refetched = resolveIconUrl("node-1", new AbortController().signal);
    expect(calls).toHaveLength(2);
    calls[1]!.resolve(signed("https://signed/two", String(clock + 300_000)));
    await expect(refetched).resolves.toBe("https://signed/two");
  });

  it("falls back to undefined when the exchange fails, and does not pin the failure", async () => {
    const { calls, drive } = createGatedDrive();
    const resolveIconUrl = createAppIconUrlResolver({ drive });

    const failed = resolveIconUrl("node-1", new AbortController().signal);
    calls[0]!.reject(new Error("signed url denied"));
    // The failure resolves to the tile, never rejects into the page.
    await expect(failed).resolves.toBeUndefined();

    // A later row mount retries: a transient 5xx must not hide a real icon
    // for the whole session. (The in-flight map only bars *simultaneous*
    // duplicates; once settled, the next caller is a new exchange.)
    const retried = resolveIconUrl("node-1", new AbortController().signal);
    expect(calls).toHaveLength(2);
    calls[1]!.resolve(signed("https://signed/recovered", String(Date.now() + 300_000)));
    await expect(retried).resolves.toBe("https://signed/recovered");
  });

  it("stops waiting when the caller aborts, without throwing", async () => {
    const { calls, drive } = createGatedDrive();
    const resolveIconUrl = createAppIconUrlResolver({ drive });

    const controller = new AbortController();
    const waiting = resolveIconUrl("node-1", controller.signal);
    controller.abort();
    await expect(waiting).resolves.toBeUndefined();

    // The abandoned exchange still warms the cache for the next mount.
    calls[0]!.resolve(signed("https://signed/late", String(Date.now() + 300_000)));
    await flush();
    await expect(resolveIconUrl("node-1", new AbortController().signal)).resolves.toBe("https://signed/late");
    expect(calls).toHaveLength(1);
  });

  it("keeps at most four exchanges in flight for a page", async () => {
    const { calls, drive } = createGatedDrive();
    const resolveIconUrl = createAppIconUrlResolver({ drive });

    const pending = ["node-1", "node-2", "node-3", "node-4", "node-5", "node-6"].map(
      (nodeId) => resolveIconUrl(nodeId, new AbortController().signal),
    );
    expect(calls).toHaveLength(4);

    // Drain the pipeline one gate at a time: each settled exchange wakes
    // exactly one queued exchange, so the ledger's fifth and sixth avatars
    // wait for a free slot instead of piling onto the wire.
    for (let index = 0; index < 6; index += 1) {
      while (calls.length <= index) await flush();
      calls[index]!.resolve(signed(`https://signed/${index}`, String(Date.now() + 300_000)));
      await flush();
    }
    const settled = await Promise.all(pending);
    expect(settled.every((url) => typeof url === "string")).toBe(true);
  });

  it("passes an explicit per-exchange timeout to the drive call", async () => {
    const { calls, drive } = createGatedDrive();
    const resolveIconUrl = createAppIconUrlResolver({ drive });

    const waiting = resolveIconUrl("node-1", new AbortController().signal);
    expect(calls[0]!.options?.timeout).toBeGreaterThan(0);
    calls[0]!.reject(new Error("timeout"));
    await expect(waiting).resolves.toBeUndefined();
  });
});
