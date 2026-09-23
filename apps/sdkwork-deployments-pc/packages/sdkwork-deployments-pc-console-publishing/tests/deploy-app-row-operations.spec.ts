/**
 * Row-operation tests for the deploy-app publishing service.
 *
 * These pin the *wire payloads* of the three operations the applications ledger
 * exposes per row (`update` / `update-source` / `publish`), using a recording
 * stub client. They exist because the contract is stricter than the generated
 * request types make obvious:
 *
 * - `releases.create` declares a **required body** `idempotencyKey` (the Rust
 *   authority rejects a blank one) *in addition to* the `Idempotency-Key`
 *   header. TypeScript reported the omission as `TS2379` — "add 'undefined' to
 *   the target's properties" — while the real defect was a missing required
 *   member, so a compile error alone is not a reliable guard here.
 * - Optional members are omitted, never sent as `undefined`, matching
 *   `exactOptionalPropertyTypes` and the wire's own reading of the two.
 */
import type { SdkworkDeployAppClient } from "@sdkwork/deployments-app-sdk";
import type { SdkworkDriveAppClient } from "@sdkwork/drive-app-sdk";
import { describe, expect, it, vi } from "vitest";
import { createDeployAppPublishingService } from "../src/service/deploy-app-publishing.ts";

/** Deterministic key source so the body/header pairing can be asserted. */
const KEY = "idem-key-0001";

interface Recorded {
  readonly appUpdate: ReturnType<typeof vi.fn>
  readonly releaseCreate: ReturnType<typeof vi.fn>
  readonly sourceCreate: ReturnType<typeof vi.fn>
}

function stubClient(recorded: Partial<Recorded> = {}): SdkworkDeployAppClient {
  const releaseCreate = recorded.releaseCreate ?? vi.fn().mockResolvedValue({ id: "release-1" })
  const appUpdate = recorded.appUpdate ?? vi.fn().mockResolvedValue({ id: "app-1" })
  const sourceCreate = recorded.sourceCreate ?? vi.fn().mockResolvedValue({ id: "source-1" })
  const client = {
    app: {
      update: appUpdate,
      platformTargets: {
        list: vi.fn().mockResolvedValue({ items: [{ id: "target-1", targetKey: "pc-web", platform: "WEB" }] }),
      },
      sourceRepositories: { create: sourceCreate },
    },
    package: { list: vi.fn().mockResolvedValue({ items: [{ id: "pkg-1" }] }) },
    release: { create: releaseCreate },
  }
  return client as unknown as SdkworkDeployAppClient
}

function serviceOf(recorded: Partial<Recorded> = {}) {
  // A *sequence*, not a constant: if an operation mints a second key anywhere,
  // the body and the header stop matching and the assertion below catches it.
  // A constant generator would hide exactly the defect being guarded.
  let minted = 0
  return createDeployAppPublishingService({
    deployClient: stubClient(recorded),
    driveClient: {} as unknown as SdkworkDriveAppClient,
    createIdempotencyKey: () => `idem-key-${String(++minted).padStart(4, "0")}`,
  })
}

describe("publishAppRelease (publish row operation)", () => {
  it("carries the required idempotencyKey in the body and the same key in the header", async () => {
    const create = vi.fn().mockResolvedValue({ id: "release-1" })
    const service = serviceOf({ releaseCreate: create })

    await service.publishAppRelease("app-1", {
      packageId: "pkg-1",
      platformTargetId: "target-1",
      semanticVersion: "1.2.3",
    })

    expect(create).toHaveBeenCalledTimes(1)
    const [appId, body, params] = create.mock.calls[0] as [string, Record<string, unknown>, { idempotencyKey: string }]
    expect(appId).toBe("app-1")
    // The body member is what the server validates; the header alone is not enough.
    expect(body.idempotencyKey).toBe(KEY)
    expect(params.idempotencyKey).toBe(KEY)
    expect(body).not.toHaveProperty("releaseNotes")
  })

  it("trims the semantic version and omits blank release notes", async () => {
    const create = vi.fn().mockResolvedValue({ id: "release-1" })
    const service = serviceOf({ releaseCreate: create })

    await service.publishAppRelease("app-1", {
      packageId: "pkg-1",
      platformTargetId: "target-1",
      semanticVersion: "  1.2.3  ",
      releaseNotes: "   ",
    })

    const [, body] = create.mock.calls[0] as [string, Record<string, unknown>]
    expect(body.semanticVersion).toBe("1.2.3")
    expect(body).not.toHaveProperty("releaseNotes")
  })

  it("wraps non-empty release notes in the free-form JSONB object", async () => {
    const create = vi.fn().mockResolvedValue({ id: "release-1" })
    const service = serviceOf({ releaseCreate: create })

    await service.publishAppRelease("app-1", {
      packageId: "pkg-1",
      platformTargetId: "target-1",
      semanticVersion: "1.2.3",
      releaseNotes: "  first cut  ",
    })

    const [, body] = create.mock.calls[0] as [string, Record<string, unknown>]
    expect(body.releaseNotes).toEqual({ note: "first cut" })
  })
});

describe("updateApp (update row operation)", () => {
  it("sends the trimmed name and description", async () => {
    const update = vi.fn().mockResolvedValue({ id: "app-1" })
    const service = serviceOf({ appUpdate: update })

    await service.updateApp("app-1", { name: "  Renamed  ", description: " note " })

    expect(update).toHaveBeenCalledWith("app-1", { name: "Renamed", description: "note" })
  });

  it("omits a blank name instead of sending an empty one", async () => {
    const update = vi.fn().mockResolvedValue({ id: "app-1" })
    const service = serviceOf({ appUpdate: update })

    await service.updateApp("app-1", { name: "   ", description: "" })

    // An empty name would be a server-side validation error; a blank description
    // is a deliberate clear, so it is still sent.
    expect(update).toHaveBeenCalledWith("app-1", { description: "" })
  })
});

describe("createSourceRepository (update-source row operation)", () => {
  it("omits a blank default branch and both optional members when unset", async () => {
    const create = vi.fn().mockResolvedValue({ id: "source-1" })

    await serviceOf({ sourceCreate: create }).createSourceRepository("app-1", {
      repoKey: " app ",
      repoProvider: "GITHUB",
      repoUrl: " https://example.test/a.git ",
      defaultBranch: "  ",
    })

    const [appId, body] = create.mock.calls[0] as [string, Record<string, unknown>]
    expect(appId).toBe("app-1")
    expect(body.repoKey).toBe("app")
    expect(body.repoUrl).toBe("https://example.test/a.git")
    expect(body).not.toHaveProperty("defaultBranch")
    expect(body).not.toHaveProperty("cloneMode")
  })
});

describe("publish option lists", () => {
  it("unwraps platform targets and packages from their page envelopes", async () => {
    const service = serviceOf()

    await expect(service.listPlatformTargets("app-1")).resolves.toEqual([
      { id: "target-1", targetKey: "pc-web", platform: "WEB" },
    ])
    await expect(service.listPackages("app-1")).resolves.toEqual([{ id: "pkg-1" }])
  })
});
