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
  MAX_CUSTOM_DOMAINS,
  createDeployAppOperationsService,
  customDomainCapacity,
  domainStatusLabel,
  inferDomainZone,
  isHostnameShaped,
  normalizeHostname,
  relativeNameInZone,
  compositionKey,
} from "../src/service/deploy-app-operations.ts";
import { environmentTabsInUse, filterDomainsByEnvironment } from "../src/components/AppDomainDialog.tsx";
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

describe("inferDomainZone", () => {
  const zone = (apexHostname: string, id = `zone-${apexHostname}`, status = "ACTIVE") =>
    ({ id, apexHostname, status }) as never

  it("matches the zone that owns the hostname, not just any zone", () => {
    const zones = [zone("example.com"), zone("other.test")]
    expect(inferDomainZone("app.example.com", zones)?.apexHostname).toBe("example.com")
    expect(inferDomainZone("app.other.test", zones)?.apexHostname).toBe("other.test")
    expect(inferDomainZone("app.nowhere.dev", zones)).toBeUndefined()
  })

  it("prefers the longest apex so a nested zone is not shadowed", () => {
    // With both zones registered, `app.eu.example.com` belongs to
    // `eu.example.com`. Claiming it under `example.com` would compute the wrong
    // relative name and the server would refuse or mis-record it.
    const zones = [zone("example.com"), zone("eu.example.com")]
    expect(inferDomainZone("app.eu.example.com", zones)?.apexHostname).toBe("eu.example.com")
    // A hostname outside the nested zone still falls back to the broader one.
    expect(inferDomainZone("app.us.example.com", zones)?.apexHostname).toBe("example.com")
  })

  it("is insensitive to case and a trailing dot", () => {
    const zones = [zone("Example.COM")]
    expect(inferDomainZone("App.Example.com.", zones)?.apexHostname).toBe("Example.COM")
    expect(normalizeHostname("  App.Example.COM.  ")).toBe("app.example.com")
  })

  it("does not treat a lookalike suffix as belonging to the zone", () => {
    // `notexample.com` must not match zone `example.com`.
    const zones = [zone("example.com")]
    expect(inferDomainZone("app.notexample.com", zones)).toBeUndefined()
  })

  it("matches the apex itself, for a bare-domain custom hostname", () => {
    const zones = [zone("example.com")]
    expect(inferDomainZone("example.com", zones)?.apexHostname).toBe("example.com")
  })
})

describe("isHostnameShaped", () => {
  it("accepts dotted lowercase names and rejects malformed ones", () => {
    expect(isHostnameShaped("app.example.com")).toBe(true)
    expect(isHostnameShaped("a-b.example.com")).toBe(true)
    // Normalization is applied first, so uppercase and a trailing dot are fine.
    expect(isHostnameShaped("App.Example.com.")).toBe(true)
    expect(isHostnameShaped("localhost")).toBe(false)
    expect(isHostnameShaped("-bad.example.com")).toBe(false)
    expect(isHostnameShaped("bad-.example.com")).toBe(false)
    expect(isHostnameShaped("")).toBe(false)
  })
})

describe("bindCustomHostname", () => {
  it("resolves the zone from registered zones when the caller omits one", async () => {
    const ensure = vi.fn(async (zoneId: string) => ({
      items: [{
        hostname: {
          id: "dom-1",
          zoneId,
          hostname: "app.example.com",
          relativeName: "app",
          status: "PENDING",
        },
      }],
    }))
    const list = vi.fn(async () => ({
      items: [
        { id: "zone-wide", apexHostname: "example.com", status: "ACTIVE" },
        { id: "zone-eu", apexHostname: "eu.example.com", status: "ACTIVE" },
      ],
    }))
    const service = createDeployAppOperationsService({
      deployClient: {
        app: {
          retrieve: async () => appFixture({ version: "3" }),
          domains: { list: async () => ({ items: [] }) },
          composition: { update: async () => ({}) },
        },
        domain: { domainZones: { list, hostnameClaims: { ensure } } },
      } as never,
      driveClient: {} as never,
      createIdempotencyKey: () => "idem",
    })

    await service.bindCustomHostname("app-1", { hostname: "app.eu.example.com" })

    // The claim must go to the *nested* zone, not the first one listed.
    expect(ensure.mock.calls[0]?.[0]).toBe("zone-eu")
  })

  it("fails with an actionable message when no registered zone owns the hostname", async () => {
    const service = createDeployAppOperationsService({
      deployClient: {
        app: {
          retrieve: async () => appFixture({ version: "3" }),
          domains: { list: async () => ({ items: [] }) },
        },
        domain: {
          domainZones: {
            list: async () => ({ items: [{ id: "z1", apexHostname: "example.com", status: "ACTIVE" }] }),
            hostnameClaims: { ensure: async () => ({ items: [] }) },
          },
        },
      } as never,
      driveClient: {} as never,
      createIdempotencyKey: () => "idem",
    })

    await expect(
      service.bindCustomHostname("app-1", { hostname: "app.unregistered.dev" }),
    ).rejects.toThrow(/No registered domain zone owns app\.unregistered\.dev/)
  })

  it("ignores zones that are not ACTIVE", async () => {
    const ensure = vi.fn(async () => ({ items: [] }))
    const service = createDeployAppOperationsService({
      deployClient: {
        app: {
          retrieve: async () => appFixture({ version: "3" }),
          domains: { list: async () => ({ items: [] }) },
        },
        domain: {
          domainZones: {
            list: async () => ({ items: [{ id: "z1", apexHostname: "example.com", status: "PENDING" }] }),
            hostnameClaims: { ensure },
          },
        },
      } as never,
      driveClient: {} as never,
      createIdempotencyKey: () => "idem",
    })

    await expect(
      service.bindCustomHostname("app-1", { hostname: "app.example.com" }),
    ).rejects.toThrow(/No registered domain zone owns/)
    expect(ensure).not.toHaveBeenCalled()
  })
})

describe("replaceDomainBindings", () => {
  it("commits exactly the given bindings with the read version as If-Match", async () => {
    const compositionUpdate = vi.fn(async () => ({}))
    const service = createDeployAppOperationsService({
      deployClient: {
        app: { composition: { update: compositionUpdate } },
      } as never,
      driveClient: {} as never,
      createIdempotencyKey: () => "idem",
    })

    await service.replaceDomainBindings("app-1", {
      version: "12",
      bindings: [
        { key: "keep-example-com", domainId: "dom-keep", pathPrefix: "/", action: { type: "SERVE" } },
      ],
    })

    const call = compositionUpdate.mock.calls[0] as unknown as [string, { bindings: { domainId: string }[] }, { ifMatch: string }]
    expect(call[0]).toBe("app-1")
    expect(call[1].bindings.map((binding) => binding.domainId)).toEqual(["dom-keep"])
    // Without If-Match a concurrent edit would be silently reverted.
    expect(call[2].ifMatch).toBe("12")
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


describe("customDomainCapacity", () => {
  it("caps the sum of bound and draft domains, not just drafts", () => {
    // Two already bound plus three open drafts is exactly the limit: the
    // operator must not be able to open a sixth slot by ignoring the bound ones.
    expect(customDomainCapacity(2, 3)).toEqual({ used: 5, remaining: 0, atCapacity: true })
    // One fewer draft leaves exactly one slot.
    expect(customDomainCapacity(2, 2)).toEqual({ used: 4, remaining: 1, atCapacity: false })
  })

  it("reports room to add while under the cap", () => {
    const fresh = customDomainCapacity(0, 0)
    expect(fresh.used).toBe(0)
    expect(fresh.remaining).toBe(MAX_CUSTOM_DOMAINS)
    expect(fresh.atCapacity).toBe(false)
  })

  it("clamps remaining at zero instead of going negative", () => {
    // Servers can return more custom domains than the product cap (the cap is
    // client-side only), and the counter must never render a negative number.
    const over = customDomainCapacity(7, 0)
    expect(over.used).toBe(7)
    expect(over.remaining).toBe(0)
    expect(over.atCapacity).toBe(true)
  })

  it("defaults to the product cap and honours an explicit override", () => {
    expect(MAX_CUSTOM_DOMAINS).toBe(5)
    expect(customDomainCapacity(0, 1).remaining).toBe(4)
    expect(customDomainCapacity(0, 1, 1).atCapacity).toBe(true)
  })
})

describe("filterDomainsByEnvironment / environmentTabsInUse", () => {
  const rows = [
    { hostname: "app.example.com", environment: "production" },
    { hostname: "app-dev.example.com", environment: "development" },
    { hostname: "app-test.example.com", environment: "test" },
    { hostname: "app-dev2.example.com", environment: "development" },
  ]

  it("returns every domain under the all tab, without copying", () => {
    expect(filterDomainsByEnvironment(rows, "all")).toBe(rows)
  })

  it("keeps only the matching environment", () => {
    const dev = filterDomainsByEnvironment(rows, "development")
    expect(dev.map((row) => row.hostname)).toEqual([
      "app-dev.example.com",
      "app-dev2.example.com",
    ])
  })

  it("returns an empty list for an environment with no domains", () => {
    expect(filterDomainsByEnvironment(rows, "staging")).toEqual([])
  })

  it("lists only environments that actually have domains, in lifecycle order", () => {
    // staging and demo are absent, so they must NOT get a tab — a tab that
    // always renders empty reads as a bug to the operator.
    expect(environmentTabsInUse(rows)).toEqual(["development", "test", "production"])
  })

  it("sorts known environments first and keeps unknown ones last", () => {
    // The server's environment field is a free string, so an unrecognised
    // value must still be reachable rather than silently dropped.
    const mixed = [
      { hostname: "a", environment: "canary" },
      { hostname: "b", environment: "production" },
      { hostname: "c", environment: "development" },
    ]
    expect(environmentTabsInUse(mixed)).toEqual(["development", "production", "canary"])
  })

  it("does not duplicate a tab when one environment repeats", () => {
    expect(environmentTabsInUse(rows).filter((env) => env === "development")).toHaveLength(1)
  })

  it("yields no tabs for an empty domain list", () => {
    expect(environmentTabsInUse([])).toEqual([])
  })
})

