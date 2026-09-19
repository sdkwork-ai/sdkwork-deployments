/**
 * Unit tests for the app row operations service and the domain column's
 * hostname derivation.
 *
 * What is worth locking down here:
 *
 * - **`primaryHostname`** must reproduce the server's rule exactly
 *   (`<label>.app.<suffix>`, see `sdkwork-deploy-core::default_app_hostname`).
 *   The list column renders it without a request, so a drift between the two
 *   would show an operator a hostname that does not resolve.
 * - **`saveDomainConfig` null-vs-absent.** `apps.update` reads an explicit
 *   `null` as "clear the override" and an absent field as "leave it alone".
 *   Getting that wrong silently wipes an operator's configured appId on an
 *   unrelated save, so the body's exact keys are asserted.
 * - **`archiveChecksum`** is a real WebCrypto digest and is asserted against a
 *   known-answer vector rather than a length check.
 * - **`bindCustomHostname`** must seed the replacement composition from the
 *   app's existing bindings, or binding one hostname silently unbinds every
 *   other one.
 */
import { describe, expect, it, vi } from "vitest";
import type { AppResponse } from "@sdkwork/deployments-app-sdk";
import {
  DEPLOY_PACKAGE_TYPE_OPTIONS,
  createDeployAppOperationsService,
  domainStatusLabel,
  relativeNameInZone,
  compositionKey,
} from "../src/service/deploy-app-operations.ts";
import { primaryHostname } from "../src/components/PublishingAppsPage.tsx";

/** Minimal AppResponse; only the fields the derivation reads matter here. */
function appFixture(overrides: Partial<AppResponse> = {}): AppResponse {
  return {
    id: "app-1",
    name: "Demo",
    slug: "demo",
    appKind: "SPA_WEB",
    appStatus: "DRAFT",
    defaultEnvironment: "production",
    createdAt: "2026-01-01T00:00:00Z",
    updatedAt: "2026-01-01T00:00:00Z",
    version: "1",
    ...overrides,
  } as AppResponse;
}

describe("primaryHostname", () => {
  it("uses the effective label and first suffix with the bare `app` label", () => {
    expect(primaryHostname(appFixture({
      appDomainLabel: "my-store",
      appDomainSuffixes: ["sdkwork.com", "sdkwork.cn"],
    }))).toBe("my-store.app.sdkwork.com")
  })

  it("falls back to the slug when no label override is set", () => {
    expect(primaryHostname(appFixture({
      slug: "demo",
      appDomainSuffixes: ["sdkwork.com"],
    }))).toBe("demo.app.sdkwork.com")
  })

  it("returns undefined when the suffix catalog is absent or empty", () => {
    expect(primaryHostname(appFixture({ appDomainLabel: "demo" }))).toBeUndefined()
    expect(primaryHostname(appFixture({ appDomainLabel: "demo", appDomainSuffixes: [] }))).toBeUndefined()
  })

  it("returns undefined rather than a degenerate hostname when the label is empty", () => {
    // An empty label would produce ".app.sdkwork.com", which resolves to nothing
    // and reads as a bug in the UI. Better to render "not configured".
    expect(primaryHostname(appFixture({ appDomainLabel: "", appDomainSuffixes: ["sdkwork.com"] })))
      .toBeUndefined()
  })
})

describe("DEPLOY_PACKAGE_TYPE_OPTIONS", () => {
  it("mirrors the Rust package-type codes 1..5 exactly once each", () => {
    const values = DEPLOY_PACKAGE_TYPE_OPTIONS.map((option) => option.value)
    expect(values).toEqual([1, 2, 3, 4, 5])
    expect(new Set(values).size).toBe(values.length)
  })

  it("gives every package type a positive size ceiling", () => {
    for (const option of DEPLOY_PACKAGE_TYPE_OPTIONS) {
      expect(option.maxSizeMiB).toBeGreaterThan(0)
    }
  })
})

describe("relativeNameInZone", () => {
  it("returns the label left of the apex, and `@` for the apex itself", () => {
    expect(relativeNameInZone("app.example.com", "example.com")).toBe("app")
    expect(relativeNameInZone("a.b.example.com", "example.com")).toBe("a.b")
    expect(relativeNameInZone("example.com", "example.com")).toBe("@")
  })

  it("normalises case and a trailing root dot on both sides", () => {
    expect(relativeNameInZone("App.Example.COM.", "example.com")).toBe("app")
  })

  it("returns the hostname unchanged when it is outside the zone", () => {
    // Not a valid request, but it must not silently truncate to something that
    // looks like it belongs to the zone.
    expect(relativeNameInZone("app.other.com", "example.com")).toBe("app.other.com")
  })
})

describe("compositionKey", () => {
  it("is stable for the same hostname and path, and distinguishes paths", () => {
    expect(compositionKey("App.Example.com", "/")).toBe("app-example-com")
    expect(compositionKey("app.example.com", "/shop")).toBe("app-example-com--shop")
    expect(compositionKey("app.example.com", "/")).toBe(compositionKey("APP.EXAMPLE.COM", "/"))
  })
})

describe("saveDomainConfig", () => {
  /**
   * `vi.fn` is given an explicit parameter list so `mock.calls` is a typed tuple —
   * an untyped stub makes `calls[0][1]` a type error, and casting it to the
   * expected shape would defeat the assertion's purpose.
   */
  function serviceWithCapture() {
    const update = vi.fn(
      async (_appId: string, _body: Record<string, unknown>): Promise<AppResponse> => appFixture(),
    )
    const service = createDeployAppOperationsService({
      deployClient: { app: { update } } as never,
      driveClient: {} as never,
      createIdempotencyKey: () => "idem",
    })
    return { service, update }
  }

  it("omits both fields when the caller touched neither (leave them alone)", async () => {
    const { service, update } = serviceWithCapture()
    await service.saveDomainConfig("app-1", {})
    const body = update.mock.calls[0]?.[1] ?? {}
    expect(Object.keys(body)).toHaveLength(0)
  })

  it("sends an explicit null to clear an override, not an empty string", async () => {
    const { service, update } = serviceWithCapture()
    await service.saveDomainConfig("app-1", { appDomainLabel: null })
    const body = update.mock.calls[0]?.[1] ?? {}
    expect(body).toHaveProperty("appDomainLabel", null)
    expect(body).not.toHaveProperty("appDomainSuffixes")
  })

  it("passes a concrete label and catalog through unchanged", async () => {
    const { service, update } = serviceWithCapture()
    await service.saveDomainConfig("app-1", {
      appDomainLabel: "my-store",
      appDomainSuffixes: ["sdkwork.com"],
    })
    const body = update.mock.calls[0]?.[1] ?? {}
    expect(body["appDomainLabel"]).toBe("my-store")
    expect(body["appDomainSuffixes"]).toEqual(["sdkwork.com"])
  })
})

describe("archiveChecksum", () => {
  it("matches the known SHA-256 of the bytes it is given", async () => {
    const service = createDeployAppOperationsService({
      deployClient: {} as never,
      driveClient: {} as never,
    })
    // "abc" — the canonical NIST vector.
    const file = {
      size: 3,
      slice: () => new Blob(),
      arrayBuffer: async () => new TextEncoder().encode("abc").buffer as ArrayBuffer,
    }
    expect(await service.archiveChecksum(file))
      .toBe("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad")
  })
})

describe("bindCustomHostname", () => {
  it("keeps the app's existing bindings in the replacement composition", async () => {
    const existingDomains = [
      { hostname: "keep.example.com", kind: "CUSTOM", domainId: "dom-keep", environment: "production", bindingStatus: "ACTIVE", verificationStatus: "VERIFIED" },
    ]
    const compositionUpdate = vi.fn(
      async (
        _appId: string,
        _body: { bindings: { domainId: string }[] },
        _params: { ifMatch: string },
      ) => ({}),
    )
    const ensure = vi.fn(async () => ({
      items: [{
        hostname: {
          id: "dom-new",
          zoneId: "zone-1",
          hostname: "new.example.com",
          relativeName: "new",
          status: "PENDING",
        },
      }],
    }))

    const service = createDeployAppOperationsService({
      deployClient: {
        app: {
          retrieve: async () => appFixture({ version: "7" }),
          domains: { list: async () => ({ items: existingDomains }) },
          composition: { update: compositionUpdate },
        },
        domain: { domainZones: { hostnameClaims: { ensure } } },
      } as never,
      driveClient: {} as never,
      createIdempotencyKey: () => "idem",
    })

    const result = await service.bindCustomHostname("app-1", {
      zoneId: "zone-1",
      apexHostname: "example.com",
      hostname: "new.example.com",
    })

    expect(result.hostname.id).toBe("dom-new")
    const call = compositionUpdate.mock.calls[0]
    expect(call).toBeDefined()
    const body = call?.[1]
    const params = call?.[2]
    const domainIds = (body?.bindings ?? []).map((binding) => binding.domainId)
    expect(domainIds).toContain("dom-keep")
    expect(domainIds).toContain("dom-new")
    expect(params?.ifMatch).toBe("7")
  })
})

describe("listDriveArchives", () => {
  it("keeps only files and coerces the string content length to a number", async () => {
    const service = createDeployAppOperationsService({
      deployClient: {} as never,
      driveClient: {
        drive: {
          search: {
            list: async () => ({
              items: [
                { id: "n1", spaceId: "s1", nodeType: "file", nodeName: "web.zip", contentType: "application/zip", contentLength: "2048", updatedAt: "2026-01-01T00:00:00Z" },
                { id: "n2", spaceId: "s1", nodeType: "folder", nodeName: "builds", updatedAt: "2026-01-01T00:00:00Z" },
                { id: "n3", spaceId: "s1", nodeType: "file", nodeName: "no-type.zip", updatedAt: "2026-01-01T00:00:00Z" },
              ],
            }),
          },
        },
      } as never,
    })

    const archives = await service.listDriveArchives()
    expect(archives.map((archive) => archive.nodeId)).toEqual(["n1", "n3"])
    expect(archives[0]?.contentLength).toBe(2048)
    // A node without a content type is still usable as an archive.
    expect(archives[1]?.contentType).toBe("application/zip")
  })
})

describe("domainStatusLabel", () => {
  /** A translator that echoes the key, so the assertion pins the *mapping*. */
  const echo = (key: string) => `t:${key}`

  it("localizes the binding axis instead of leaking the raw enum", () => {
    // The regression: `绑定: ACTIVE` sitting beside `DNS: 无需验证`.
    expect(domainStatusLabel("binding", "ACTIVE", echo)).toBe("t:domainBindingActive")
    expect(domainStatusLabel("binding", "PENDING", echo)).toBe("t:domainBindingPending")
    expect(domainStatusLabel("binding", "PAUSED", echo)).toBe("t:domainBindingPaused")
    expect(domainStatusLabel("binding", "FAILED", echo)).toBe("t:domainBindingFailed")
    expect(domainStatusLabel("binding", "ARCHIVED", echo)).toBe("t:domainBindingArchived")
  })

  it("localizes the verification axis", () => {
    expect(domainStatusLabel("verification", "NOT_REQUIRED", echo)).toBe("t:domainVerificationNotRequired")
    expect(domainStatusLabel("verification", "EXPIRED", echo)).toBe("t:domainVerificationExpired")
  })

  it("lowercases the environment axis before lookup", () => {
    expect(domainStatusLabel("environment", "production", echo)).toBe("t:domainEnvProduction")
    expect(domainStatusLabel("environment", "DEVELOPMENT", echo)).toBe("t:domainEnvDevelopment")
  })

  it("falls through unknown values rather than hiding a new server enum", () => {
    expect(domainStatusLabel("binding", "SOMETHING_NEW", echo)).toBe("SOMETHING_NEW")
    expect(domainStatusLabel("environment", "canary", echo)).toBe("canary")
  })

  it("maps every enum value the contract declares", () => {
    // Any value that would survive as a raw token is a missed translation.
    const axes: Array<["binding" | "verification", string[]]> = [
      ["binding", ["PENDING", "VERIFIED", "ACTIVE", "PAUSED", "FAILED", "ARCHIVED"]],
      ["verification", ["NOT_REQUIRED", "PENDING", "VERIFIED", "FAILED", "EXPIRED"]],
    ]
    const leaked: string[] = []
    for (const [axis, values] of axes) {
      for (const value of values) {
        const rendered = domainStatusLabel(axis, value, echo)
        if (rendered === value) leaked.push(`${axis}:${value}`)
      }
    }
    expect(leaked).toEqual([])
  })
})
