/**
 * The application avatar's three ingredients, and the one network trip behind it.
 *
 * The ledger's identity cell leads with a 32px tile. That tile is drawn from
 * three sources, in strict fallback order:
 *
 *  1. **A deterministic gradient.** {@link appAvatarGradientClass} hashes the
 *     app slug (FNV-1a — stable across renders, sessions and machines) into a
 *     fixed palette, the way GitHub picks identicon hues and Linear picks
 *     project colors. Two goals: every row is distinguishable at a glance
 *     before a single word is read, and the same app is always the same color,
 *     so re-sorting the ledger never makes two rows swap palettes.
 *  2. **The application-kind glyph.** {@link appAvatarIconKey} reuses the
 *     publish dialog's `DeployAppTypeIcon` vocabulary, so the tile says what
 *     the app *is* (web / mini-program / Android / …) rather than spelling its
 *     first letter — an avatar that repeats information the row already prints
 *     is decoration, one that adds a dimension is an interface. `SPA_WEB`
 *     covers both web ends, so when the publish flow recorded
 *     `metadata.surface` the glyph follows it (h5 phone vs pc monitor);
 *     unpublished apps fall back to the pc glyph, matching the facet's
 *     "SPA_WEB speaks for both ends" rule in `app-list-facets.ts`.
 *  3. **The uploaded store icon.** The create/publish flow persists
 *     `metadata.media.icon` as a Drive node reference — deliberately *without*
 *     a URL, because download URLs are signed and expire
 *     (`CreateDownloadUrlResponse.expiresAtEpochMs`). {@link
 *     createAppIconUrlResolver} exchanges a reference for a fresh signed URL
 *     and owns the economics of doing that from a list: one cache keyed by
 *     node id, in-flight deduplication so a re-render never re-requests,
 *     bounded concurrency so a hundred-row ledger cannot open a hundred
 *     requests at once, a per-request timeout, and a TTL that retires a URL
 *     *before* its signature lapses (an `<img>` that starts failing mid-session
 *     is worse than never showing the image). Every failure mode resolves to
 *     `undefined`, and the caller renders the gradient tile — the fallback is
 *     not an error state, it is the design.
 *
 * The resolver returns promises that never reject by contract: a missing,
 * expired or denied icon is a *presentation* fact (show the tile), not a
 * command the operator issued, so it must not land in the page's error banner
 * next to real failures like a failed pause.
 */
import type {
  AppKind,
  AppResponse,
  CreateDownloadUrlResponse,
} from "@sdkwork/deployments-pc-console-core/sdk";
import type { DeployAppTypeIconId } from "./deploy-app-publishing.ts";

/** Payload shape of the tile's fallback layers; every member is renderable without the network. */
export type AppAvatarSubject = Pick<AppResponse, "slug" | "appKind" | "metadata">;

/** Size of the deterministic gradient palette. Keeping it a named constant ties the CSS to the hash. */
export const APP_AVATAR_GRADIENT_COUNT = 10;

/**
 * FNV-1a over the seed's UTF-16 code units.
 *
 * A cryptographic hash would be wasted work: the only requirement is that the
 * same slug always lands on the same palette index and nearby slugs do not
 * systematically collide. FNV-1a is the standard cheap answer, and its
 * distribution over the low bits is good enough that the ten buckets stay
 * evenly visited across a real ledger.
 */
export function hashAppAvatarSeed(seed: string): number {
  let hash = 0x811c9dc5;
  for (let index = 0; index < seed.length; index += 1) {
    hash ^= seed.charCodeAt(index);
    hash = Math.imul(hash, 0x01000193);
  }
  return hash >>> 0;
}

/**
 * The gradient class for one app's tile.
 *
 * Seeded by `slug` (unique per tenant) rather than `name` (renameable) or `id`
 * (absent before the server answers): the class must be computable from data
 * the row always carries, and stable across renames is less valuable than
 * stable across *rows*, which `slug` gives for free. Falls back to the empty
 * string for an empty seed so the caller never renders a class that the CSS
 * does not define.
 */
export function appAvatarGradientClass(seed: string): string {
  if (seed === "") return ""
  return `apps-avatar-gradient-${hashAppAvatarSeed(seed) % APP_AVATAR_GRADIENT_COUNT}`
}

/**
 * `AppKind` → glyph id, exhaustive on purpose: a kind the contract adds must
 * break compilation here and pick a glyph in the same change, exactly the rule
 * `APP_KIND_LABEL_KEYS` already establishes for copy. The glyph vocabulary is
 * the publish dialog's (`DeployAppTypeIconId`), so a type the dialog can draw
 * is always drawable on the ledger.
 */
const APP_AVATAR_ICON_KEYS: Readonly<Record<AppKind, DeployAppTypeIconId>> = {
  STATIC_WEB: "static",
  SPA_WEB: "pc",
  API_SERVICE: "api",
  WECHAT_MINIPROGRAM: "mini-program",
  DOUYIN_MINIPROGRAM: "mini-program",
  IOS_APP: "ios",
  ANDROID_APP: "android",
  HARMONYOS_APP: "harmony",
  DESKTOP_APP: "desktop",
};

/** Structural read of `metadata.surface` — the publish flow's web-end marker ("h5" | "pc" | …). */
function readMetadataSurface(metadata: unknown): string | undefined {
  if (!isRecord(metadata)) return undefined
  const surface = metadata["surface"]
  return typeof surface === "string" && surface !== "" ? surface : undefined
}

/** Minimal record guard — the only `unknown`-narrowing the parser needs. */
function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null
}

/**
 * The glyph for one row's tile.
 *
 * `SPA_WEB` is the one kind that spans two glyphs: the publish flow records
 * which web end it targeted in `metadata.surface`, and the tile follows that
 * when present (h5 → phone, anything else → monitor). Everything else maps
 * one-to-one through the exhaustive table, so a contract addition fails to
 * compile instead of silently rendering the pc glyph forever.
 */
export function appAvatarIconKey(app: AppAvatarSubject): DeployAppTypeIconId {
  if (app.appKind === "SPA_WEB" && readMetadataSurface(app.metadata) === "h5") {
    return "h5"
  }
  return APP_AVATAR_ICON_KEYS[app.appKind]
}

/**
 * Extract the Drive node id of the uploaded store icon, if the row carries one.
 *
 * `metadata` arrives from the wire as `Record<string, unknown>` (an echo of the
 * server's JSONB), so the media group is *parsed*, not cast: every hop is a
 * structural check and the answer is either a non-empty node id or nothing.
 * A malformed reference degrades to "no icon", which the tile already renders
 * — the fallback absorbs contract drift from older clients that wrote other
 * shapes.
 */
export function readAppIconDriveNodeId(metadata: unknown): string | undefined {
  if (!isRecord(metadata)) return undefined
  if (!isRecord(metadata["media"])) return undefined
  const icon = metadata["media"]["icon"]
  if (!isRecord(icon)) return undefined
  const driveNodeId = icon["driveNodeId"]
  return typeof driveNodeId === "string" && driveNodeId !== "" ? driveNodeId : undefined
}

/**
 * The one Drive capability the avatar needs: exchange a node id for a signed
 * URL. Narrow on purpose — the resolver (and, more importantly, its tests)
 * depend on this shape and nothing else of the injected client. The params and
 * options are spelled structurally (they mirror
 * `DriveNodesDownloadUrlsRetrieveParams` / the request options the generated
 * API takes) instead of imported, because the generated package root re-exports
 * the response type but not those two helpers, and the console packages' import
 * boundary is the console-core mirror.
 */
export interface AppIconDownloadUrlsApi {
  retrieve(
    nodeId: string,
    params?: { requestedTtlSeconds?: number },
    requestOptions?: { signal?: AbortSignal; timeout?: number },
  ): Promise<CreateDownloadUrlResponse>
}

/** The Drive aggregate face (`client.drive`) the resolver reads. */
export interface AppIconDriveClient {
  readonly nodes: { readonly downloadUrls: AppIconDownloadUrlsApi }
}

export interface AppIconUrlResolverOptions {
  /**
   * The Drive aggregate the page already holds (`driveClient.drive`). The page
   * passes the whole client's aggregate; this contract reads only
   * `nodes.downloadUrls.retrieve`. The narrow face keeps the dependency honest
   * and the tests free of casts.
   */
  readonly drive: AppIconDriveClient
  /** Injectable clock for tests; wall clock in production. */
  readonly now?: () => number
}

/** Retire a signed URL this long before its signature actually lapses. */
const ICON_URL_TTL_SAFETY_MS = 60_000

/** Hard ceiling on one download-URL exchange; the caller renders the tile on timeout. */
const ICON_URL_TIMEOUT_MS = 8_000

/** A ledger page never opens more than this many exchanges at once. */
const ICON_URL_MAX_CONCURRENT = 4

interface CachedIconUrl {
  readonly url: string
  readonly expiresAtMs: number
}

/**
 * Build the avatar's URL resolver: one cache per page instance.
 *
 * The returned function never rejects and never hangs: a failed, timed-out or
 * aborted exchange resolves `undefined`, and the *negative* result is
 * deliberately not cached — a transient 5xx must not pin the fallback tile for
 * the whole session, and the in-flight map already prevents a visible row from
 * hammering the endpoint. Abort semantics are caller-scoped: an aborted caller
 * stops waiting immediately, while the shared exchange it may have started
 * runs on to warm the cache for the next mount.
 */
export function createAppIconUrlResolver(
  options: AppIconUrlResolverOptions,
): (nodeId: string, signal: AbortSignal) => Promise<string | undefined> {
  const driveApi: AppIconDriveClient = options.drive
  const now = options.now ?? (() => Date.now())
  const cache = new Map<string, CachedIconUrl>()
  const inFlight = new Map<string, Promise<string | undefined>>()
  let activeExchanges = 0
  const waiting: (() => void)[] = []

  const release = () => {
    const next = waiting.shift()
    if (next !== undefined) {
      next()
      return
    }
    activeExchanges -= 1
  }

  const exchange = async (nodeId: string): Promise<string | undefined> => {
    // A free slot is taken synchronously so the first four exchanges fire in
    // the caller's tick (an effect resolving an avatar should reach the wire
    // immediately, not a microtask later); only a saturated page queues.
    if (activeExchanges >= ICON_URL_MAX_CONCURRENT) {
      await new Promise<void>((resolveAcquire) => {
        waiting.push(() => {
          activeExchanges += 1
          resolveAcquire()
        })
      })
    } else {
      activeExchanges += 1
    }
    try {
      const response = await driveApi.nodes.downloadUrls.retrieve(nodeId, undefined, {
        timeout: ICON_URL_TIMEOUT_MS,
      })
      const url = typeof response.downloadUrl === "string" && response.downloadUrl !== ""
        ? response.downloadUrl
        : undefined
      const expiresAtMs = Number(response.expiresAtEpochMs)
      if (url !== undefined && Number.isFinite(expiresAtMs)) {
        cache.set(nodeId, { url, expiresAtMs })
      }
      return url
    } catch {
      // The fallback *is* the design: a denied, deleted or timed-out icon
      // renders the gradient tile. Recorded here because a bare catch reads as
      // an accident — this one is the resolver's contract (never rejects).
      return undefined
    } finally {
      release()
    }
  }

  return (nodeId, signal) => {
    const cached = cache.get(nodeId)
    if (cached !== undefined && cached.expiresAtMs - ICON_URL_TTL_SAFETY_MS > now()) {
      return Promise.resolve(cached.url)
    }
    const pending = inFlight.get(nodeId)
    if (pending !== undefined) return raceAbort(pending, signal)
    const exchangePromise = exchange(nodeId).finally(() => {
      inFlight.delete(nodeId)
    })
    inFlight.set(nodeId, exchangePromise)
    return raceAbort(exchangePromise, signal)
  }
}

/**
 * Stop waiting when the caller aborts, without throwing into the caller.
 *
 * Abandonment must be indistinguishable from "no icon" at the call site (the
 * effect simply stops caring), so the abort resolves `undefined` rather than
 * rejecting — a rejection here would make every caller wrap try/catch around
 * a function whose contract says it never fails.
 */
function raceAbort(
  promise: Promise<string | undefined>,
  signal: AbortSignal,
): Promise<string | undefined> {
  if (signal.aborted) return Promise.resolve(undefined)
  return new Promise<string | undefined>((resolveRace) => {
    const onAbort = () => {
      resolveRace(undefined)
    }
    signal.addEventListener("abort", onAbort, { once: true })
    promise.then(
      (url) => {
        signal.removeEventListener("abort", onAbort)
        resolveRace(url)
      },
      () => {
        signal.removeEventListener("abort", onAbort)
        resolveRace(undefined)
      },
    )
  })
}
