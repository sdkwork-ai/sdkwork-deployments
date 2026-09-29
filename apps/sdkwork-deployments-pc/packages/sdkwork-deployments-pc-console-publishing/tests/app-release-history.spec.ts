/**
 * 发布历史的领域逻辑测试。
 *
 * 这个能力真正的判断只有两条 —— **哪一版在线上**、**哪一版还能回滚** —— 而两条
 * 都会因为数据里的边角（分页外的制品、无主的部署、同一毫秒的两次上传、在途部署与
 * 回滚同时存在）给出错误的答案。它们全是纯函数，所以能在这里逐条钉住；组件那边只
 * 需要证明这些结论被摆上了屏、被按下了按钮（见 `app-release-history-drawer.spec.tsx`）。
 */
import { describe, expect, it } from "vitest";
import type {
  AppDeploymentResponse,
  AppReleaseResponse,
  PackageResponse,
} from "@sdkwork/deployments-pc-console-core/sdk";
import {
  formatByteSize,
  mergeReleaseHistory,
  rollbackAvailability,
  shortDigest,
} from "../src/service/app-release-history.ts";

function release(overrides: Partial<AppReleaseResponse> & { readonly id: string }): AppReleaseResponse {
  return {
    appId: "app-1",
    platformTargetId: "target-1",
    packageId: `pkg-${overrides.id}`,
    semanticVersion: "1.0.0",
    buildNumber: "1",
    releaseStatus: "SUPERSEDED",
    createdAt: "2026-09-01T00:00:00Z",
    updatedAt: "2026-09-01T00:00:00Z",
    version: "1",
    ...overrides,
  } as AppReleaseResponse;
}

function pkg(overrides: Partial<PackageResponse> & { readonly id: string }): PackageResponse {
  return {
    appId: "app-1",
    platformTargetId: "target-1",
    buildId: "build-1",
    packageFormat: "ZIP",
    semanticVersion: "1.0.0",
    packageSizeBytes: "1048576",
    checksumSha256: "a".repeat(64),
    manifestSha256: "b".repeat(64),
    packageStatus: "READY",
    createdAt: "2026-09-01T00:00:00Z",
    updatedAt: "2026-09-01T00:00:00Z",
    version: "1",
    ...overrides,
  } as PackageResponse;
}

function deployment(overrides: Partial<AppDeploymentResponse> & { readonly id: string }): AppDeploymentResponse {
  return {
    appId: "app-1",
    environment: "production",
    deploymentStatus: "PENDING",
    deploymentKind: "ARTIFACT_RELEASE",
    deploymentTarget: "WEB_NODE",
    platformTargetId: "target-1",
    createdAt: "2026-09-01T00:00:00Z",
    updatedAt: "2026-09-01T00:00:00Z",
    version: "1",
    ...overrides,
  } as AppDeploymentResponse;
}

describe("mergeReleaseHistory", () => {
  it("joins a release to its artifact and to the deployments cut from it", () => {
    const merged = mergeReleaseHistory({
      packages: [pkg({ id: "pkg-r1" })],
      releases: [release({ id: "r1", packageId: "pkg-r1" })],
      deployments: [
        deployment({ id: "d1", releaseId: "r1", createdAt: "2026-09-01T01:00:00Z" }),
        deployment({ id: "d2", releaseId: "r1", createdAt: "2026-09-02T01:00:00Z" }),
      ],
    })
    expect(merged.entries).toHaveLength(1)
    expect(merged.entries[0]?.package?.id).toBe("pkg-r1")
    // Deployments are newest-first so "latest deployment" is index 0 everywhere.
    expect(merged.entries[0]?.deployments.map((entry) => entry.id)).toEqual(["d2", "d1"])
  })

  it("orders releases by createdAt descending, then by buildNumber descending", () => {
    const merged = mergeReleaseHistory({
      packages: [],
      releases: [
        release({ id: "old", createdAt: "2026-08-01T00:00:00Z", buildNumber: "9" }),
        release({ id: "same-time-low", createdAt: "2026-09-01T00:00:00Z", buildNumber: "40" }),
        release({ id: "same-time-high", createdAt: "2026-09-01T00:00:00Z", buildNumber: "41" }),
      ],
      deployments: [],
    })
    // Same millisecond: buildNumber breaks the tie, otherwise the answer to
    // "which release did I just cut" would flip between renders.
    expect(merged.entries.map((entry) => entry.release.id)).toEqual(["same-time-high", "same-time-low", "old"])
  })

  it("reports the serving release as current, taken from the newest serving deployment", () => {
    const merged = mergeReleaseHistory({
      packages: [],
      releases: [
        release({ id: "r1", createdAt: "2026-08-01T00:00:00Z" }),
        release({ id: "r2", createdAt: "2026-09-01T00:00:00Z" }),
      ],
      deployments: [
        deployment({ id: "d1", releaseId: "r1", deploymentStatus: "LIVE", createdAt: "2026-08-02T00:00:00Z" }),
        deployment({ id: "d2", releaseId: "r2", deploymentStatus: "PENDING", createdAt: "2026-09-02T00:00:00Z" }),
      ],
    })
    // An in-flight deployment is *not* current: saying otherwise would make the
    // operator believe the switch already happened.
    expect(merged.currentReleaseId).toBe("r1")
    expect(merged.servingDeployment?.id).toBe("d1")
  })

  it("treats DEGRADED as serving so a degraded release is not offered as a rollback target", () => {
    const merged = mergeReleaseHistory({
      packages: [],
      releases: [release({ id: "r1" })],
      deployments: [deployment({ id: "d1", releaseId: "r1", deploymentStatus: "DEGRADED" })],
    })
    expect(merged.currentReleaseId).toBe("r1")
  })

  it("keeps deployments whose release is out of the page window as orphans", () => {
    const merged = mergeReleaseHistory({
      packages: [],
      releases: [release({ id: "r1" })],
      deployments: [
        deployment({ id: "d1", releaseId: "r1" }),
        // releaseId points at a release this read never returned.
        deployment({ id: "d-unknown", releaseId: "r-missing" }),
        // No releaseId at all.
        deployment({ id: "d-null" }),
      ],
    })
    expect(merged.entries[0]?.deployments.map((entry) => entry.id)).toEqual(["d1"])
    expect(merged.orphanDeployments.map((entry) => entry.id).sort()).toEqual(["d-null", "d-unknown"])
  })

  it("marks the snapshot truncated when any one page came back full", () => {
    const packages = Array.from({ length: 200 }, (_, index) => pkg({ id: `p${index}` }))
    const merged = mergeReleaseHistory({ packages, releases: [], deployments: [] })
    expect(merged.truncated).toBe(true)
    expect(mergeReleaseHistory({ packages: [], releases: [], deployments: [] }).truncated).toBe(false)
  })

  it("leaves the artifact undefined rather than inventing one", () => {
    const merged = mergeReleaseHistory({
      packages: [],
      releases: [release({ id: "r1", packageId: "pkg-gone" })],
      deployments: [],
    })
    // packageId is required by the contract, so "no artifact" is not a state the
    // protocol can express — the honest answer is "not in this page".
    expect(merged.entries[0]?.package).toBeUndefined()
  })
})

describe("rollbackAvailability", () => {
  const archivedReason = (result: ReturnType<typeof rollbackAvailability>) =>
    result.kind === "unavailable" ? result.reason : "available"

  function build(input: {
    releases: readonly AppReleaseResponse[]
    packages?: readonly PackageResponse[]
    deployments: readonly AppDeploymentResponse[]
  }) {
    return mergeReleaseHistory({
      packages: input.packages ?? input.releases.map((entry) => pkg({ id: entry.packageId })),
      releases: input.releases,
      deployments: input.deployments,
    })
  }

  it("refuses the release that is already serving, whatever else is true", () => {
    const history = build({
      releases: [release({ id: "r1", releaseStatus: "ACTIVE", createdAt: "2026-09-01T00:00:00Z" })],
      deployments: [deployment({ id: "d1", releaseId: "r1", deploymentStatus: "LIVE" })],
    })
    expect(archivedReason(rollbackAvailability(history.entries[0]!, history))).toBe("serving")
  })

  it("refuses while a deployment of that release is still in flight", () => {
    const history = build({
      releases: [
        release({ id: "r1", releaseStatus: "ACTIVE", createdAt: "2026-08-01T00:00:00Z" }),
        release({ id: "r2", releaseStatus: "SUPERSEDED", createdAt: "2026-09-10T00:00:00Z" }),
      ],
      deployments: [
        deployment({ id: "d-live", releaseId: "r1", deploymentStatus: "LIVE" }),
        deployment({ id: "d-wait", releaseId: "r2", deploymentStatus: "PENDING_REVIEW" }),
      ],
    })
    const target = history.entries.find((entry) => entry.release.id === "r2")!
    expect(archivedReason(rollbackAvailability(target, history))).toBe("inFlight")
  })

  it("refuses a retired or archived release", () => {
    for (const status of ["RETIRED", "ARCHIVED", "DRAFT"] as const) {
      const history = build({
        releases: [
          release({ id: "r1", releaseStatus: "ACTIVE", createdAt: "2026-08-01T00:00:00Z" }),
          release({ id: "r2", releaseStatus: status, createdAt: "2026-09-10T00:00:00Z" }),
        ],
        deployments: [deployment({ id: "d-live", releaseId: "r1", deploymentStatus: "LIVE" })],
      })
      const target = history.entries.find((entry) => entry.release.id === "r2")!
      expect(archivedReason(rollbackAvailability(target, history)), status).toBe("releaseNotDeployable")
    }
  })

  it("refuses when the artifact behind the release was pulled", () => {
    const history = build({
      releases: [
        release({ id: "r1", releaseStatus: "ACTIVE", createdAt: "2026-08-01T00:00:00Z" }),
        release({ id: "r2", releaseStatus: "SUPERSEDED", packageId: "pkg-gone", createdAt: "2026-09-10T00:00:00Z" }),
      ],
      packages: [pkg({ id: "pkg-r1", packageStatus: "READY" }), pkg({ id: "pkg-gone", packageStatus: "RETIRED" })],
      deployments: [deployment({ id: "d-live", releaseId: "r1", deploymentStatus: "LIVE" })],
    })
    const target = history.entries.find((entry) => entry.release.id === "r2")!
    expect(archivedReason(rollbackAvailability(target, history))).toBe("packageNotDeployable")
  })

  it("refuses when no deployment exists to copy the target from", () => {
    const history = build({
      releases: [release({ id: "r1", releaseStatus: "SUPERSEDED" })],
      deployments: [],
    })
    expect(archivedReason(rollbackAvailability(history.entries[0]!, history))).toBe("noReferenceDeployment")
  })

  it("plans a rollback by copying the four target parameters from the live deployment", () => {
    const history = build({
      releases: [
        release({ id: "r-old", releaseStatus: "SUPERSEDED", createdAt: "2026-08-01T00:00:00Z" }),
        release({ id: "r-new", releaseStatus: "ACTIVE", createdAt: "2026-09-10T00:00:00Z" }),
      ],
      deployments: [
        deployment({
          id: "d-old",
          releaseId: "r-old",
          deploymentStatus: "ROLLED_BACK",
          deploymentKind: "SITE_CONFIG",
          deploymentTarget: "CONTAINER",
          platformTargetId: "target-old",
          environment: "staging",
        }),
        deployment({
          id: "d-live",
          releaseId: "r-new",
          deploymentStatus: "LIVE",
          deploymentKind: "ARTIFACT_RELEASE",
          deploymentTarget: "WEB_NODE",
          platformTargetId: "target-new",
          environment: "production",
        }),
      ],
    })
    const target = history.entries.find((entry) => entry.release.id === "r-old")!
    const availability = rollbackAvailability(target, history)
    expect(availability.kind).toBe("available")
    if (availability.kind !== "available") return
    // The live deployment wins over the target release's own deployment: a
    // rollback reproduces *the environment that is live*, not the historic one.
    expect(availability.plan).toMatchObject({
      releaseId: "r-old",
      platformTargetId: "target-new",
      deploymentKind: "ARTIFACT_RELEASE",
      deploymentTarget: "WEB_NODE",
      environment: "production",
      referenceDeploymentId: "d-live",
      referenceIsServing: true,
    })
  })

  it("falls back to the release's own deployment when nothing is live", () => {
    const history = build({
      releases: [release({ id: "r1", releaseStatus: "SUPERSEDED" })],
      deployments: [
        deployment({
          id: "d1",
          releaseId: "r1",
          deploymentStatus: "FAILED",
          deploymentKind: "OTA_DISTRIBUTION",
          deploymentTarget: "OTA",
          environment: "demo",
        }),
      ],
    })
    const availability = rollbackAvailability(history.entries[0]!, history)
    expect(availability.kind).toBe("available")
    if (availability.kind !== "available") return
    expect(availability.plan).toMatchObject({
      platformTargetId: "target-1",
      deploymentKind: "OTA_DISTRIBUTION",
      deploymentTarget: "OTA",
      environment: "demo",
      referenceDeploymentId: "d1",
      referenceIsServing: false,
    })
  })
})

describe("presentation helpers", () => {
  it("renders int64 byte strings without losing the unit", () => {
    expect(formatByteSize("0")).toBe("0 B")
    expect(formatByteSize("512")).toBe("512 B")
    expect(formatByteSize("1024")).toBe("1.0 KB")
    expect(formatByteSize("4404019")).toBe("4.2 MB")
    // Defensive: the field is a string on the wire, so garbage must degrade to
    // "unknown" rather than print `NaN`.
    expect(formatByteSize("not-a-number")).toBeUndefined()
    expect(formatByteSize(undefined)).toBeUndefined()
  })

  it("shortens a digest without dropping it entirely", () => {
    expect(shortDigest("a".repeat(64))).toBe(`${"a".repeat(12)}…`)
    expect(shortDigest("short")).toBe("short")
    expect(shortDigest(undefined)).toBeUndefined()
  })
})
