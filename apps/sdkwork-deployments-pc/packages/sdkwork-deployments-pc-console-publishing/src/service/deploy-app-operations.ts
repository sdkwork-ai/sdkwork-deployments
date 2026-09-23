/**
 * App row operations: upload code, publishing domains, and app detail.
 *
 * These are the three capabilities the apps list exposes per row. Like
 * `deploy-app-publishing.ts` this module is UI-framework-free: it takes the two
 * generated clients and returns plain data, so the deployments console and any
 * host that can construct the clients share one implementation.
 *
 * Design notes that are easy to get wrong:
 *
 * - **Upload reuses `createDeployApplicationPublisher`** rather than
 *   re-implementing the upload. That composed SDK already does the whole
 *   chunked Drive upload (with in-browser SHA-256 and resumable parts), the
 *   `artifacts.create` registration, and the release/deployment steps, in one
 *   abortable, progress-reporting call. A second implementation here would be a
 *   second set of bugs.
 * - **`apps.update` treats `null` as "clear the override"**, not "leave alone",
 *   so the domain dialog must send `undefined` (field absent) when the user did
 *   not touch the application id. See `deserialize_double_option` in
 *   `crates/sdkwork-deploy-contract/src/app_delivery.rs`.
 * - **A custom hostname becomes routable only after it is bound**, and the only
 *   binding surface is `apps.composition.update`. Registering the hostname alone
 *   proves ownership but does not serve traffic, so the two steps are issued
 *   together and the draft composition is seeded from the app's current
 *   bindings (read back through `apps.domains.list`).
 */
import type {
  AppCompositionResponse,
  AppDomainResponse,
  AppKind,
  AppResponse,
  AppStatus,
  ArtifactResponse,
  CreateArtifactRequest,
  CreateSourceRepositoryRequest,
  DomainHostnameResponse,
  DomainZoneResponse,
  PageInfo,
  PlatformTargetResponse,
  SourceRepositoryResponse,
  SdkworkDeployAppClient,
} from "@sdkwork/deployments-pc-console-core/sdk";
import type {
  DriveUploaderBlobLike,
  DriveUploaderProgress,
  SdkworkDriveAppClient,
} from "@sdkwork/deployments-pc-console-core/sdk";
import type {
  ApplicationPublishProgress,
  ApplicationPublishResult,
  DeployApplicationPublisher,
} from "@sdkwork/deployments-pc-console-core/sdk";
import { createDeployApplicationPublisher } from "@sdkwork/deployments-pc-console-core/sdk";
import { uuid } from "@sdkwork/utils/id";

/* ------------------------------------------------------------------ *
 * Upload code
 * ------------------------------------------------------------------ */

/** Where the code being published comes from. */
export type DeployCodeSource = "local" | "git" | "drive";

/**
 * Package types of `deploy_artifact.package_type`.
 *
 * Mirrors the values the Rust authority accepts; the labels live in the message
 * catalog so the numeric code is never shown to an operator.
 */
export interface DeployPackageTypeOption {
  readonly value: number
  readonly labelKey: import("../i18n.ts").PublishingMessageKey
  /** Upper bound the service enforces for this package type, in MiB. */
  readonly maxSizeMiB: number
}

export const DEPLOY_PACKAGE_TYPE_OPTIONS: readonly DeployPackageTypeOption[] = [
  { value: 1, labelKey: "packageTypeWebStatic", maxSizeMiB: 512 },
  { value: 2, labelKey: "packageTypeNativeApp", maxSizeMiB: 2048 },
  { value: 3, labelKey: "packageTypeMiniProgram", maxSizeMiB: 512 },
  { value: 4, labelKey: "packageTypeServerBundle", maxSizeMiB: 2048 },
  { value: 5, labelKey: "packageTypeGeneric", maxSizeMiB: 2048 },
] as const;

/** The `.zip` upload the browser performs before registering an artifact. */
export interface DeployCodeArchiveUpload {
  readonly file: DriveUploaderBlobLike
  readonly fileName: string
  readonly contentType: string
  /** SHA-256 hex digest; the publisher requires it, so it is computed here. */
  readonly checksumSha256: string
}

export interface DeployUploadCodeFromArchiveInput {
  readonly appId: string
  readonly packageType: number
  readonly archive: DeployCodeArchiveUpload
  readonly signal?: AbortSignal | undefined
  readonly onProgress?: ((progress: DeployUploadProgress) => void) | undefined
}

/**
 * Outcome of one upload. The artifact is always registered; the release and
 * deployment exist only when the caller asked for them.
 */
export interface DeployUploadCodeResult {
  readonly artifactId: string
  readonly artifact: ArtifactResponse
  readonly result: ApplicationPublishResult
}

/** Normalized upload progress, flattened out of the publisher's union. */
export interface DeployUploadProgress {
  readonly stage: string
  readonly uploading: boolean
  readonly uploadedBytes: number
  readonly totalBytes: number
  readonly uploadedParts: number
  readonly totalParts: number
  readonly errorMessage?: string | undefined
}

/** One Git repository binding request, as the dialog collects it. */
export interface DeployGitSourceInput {
  readonly repoKey: string
  readonly repoProvider: CreateSourceRepositoryRequest["repoProvider"]
  readonly repoUrl: string
  readonly defaultBranch?: string | undefined
  readonly cloneMode?: CreateSourceRepositoryRequest["cloneMode"] | undefined
  readonly credentialSecretRef?: string | undefined
}

/** A Drive node the operator can reuse as a code archive. */
export interface DeployDriveArchiveOption {
  readonly nodeId: string
  readonly spaceId: string
  readonly fileName: string
  readonly contentType: string
  readonly contentLength: number
  readonly updatedAt: string
}

/* ------------------------------------------------------------------ *
 * Domains
 * ------------------------------------------------------------------ */

/** One hostname of the app, normalized for the domain column and the dialog. */
export type DeployAppDomain = AppDomainResponse

/** The app's editable publishing-domain configuration plus its live hostnames. */
export interface DeployAppDomainState {
  readonly app: AppResponse
  readonly domains: readonly DeployAppDomain[]
  /** `true` once the app has at least one provisioned platform hostname. */
  readonly provisioned: boolean
}

/** A hostname the operator wants to serve on the app. */
export interface DeployCustomHostnameInput {
  /**
   * The zone to claim the hostname under. Optional: when omitted, the service
   * resolves it from the registered zones by longest-apex match
   * (`inferDomainZone`), which is what lets the operator type a bare domain.
   */
  readonly zoneId?: string | undefined
  readonly apexHostname?: string | undefined
  /** Full hostname, e.g. `app.example.com`. */
  readonly hostname: string
}

export interface DeployCustomHostnameResult {
  readonly hostname: DomainHostnameResponse
  /** The composition revision the binding was committed in. */
  readonly composition?: AppCompositionResponse | undefined
}

/** One route the composition should serve. */
export interface DeployBindingInput {
  readonly key: string
  readonly domainId: string
  readonly pathPrefix: string
  readonly action: { readonly type: "SERVE" }
}

/** The complete binding set to commit, plus the version it was derived from. */
export interface DeployReplaceBindingsInput {
  /** `AppResponse.version` the bindings were read at; sent as `If-Match`. */
  readonly version: string
  readonly bindings: readonly DeployBindingInput[]
}

/* ------------------------------------------------------------------ *
 * Detail
 * ------------------------------------------------------------------ */

/** Everything the detail drawer renders, gathered from four endpoints. */
export interface DeployAppDetail {
  readonly app: AppResponse
  readonly domains: readonly DeployAppDomain[]
  readonly platformTargets: readonly PlatformTargetResponse[]
  readonly sourceRepositories: readonly SourceRepositoryResponse[]
}

/* ------------------------------------------------------------------ *
 * Service
 * ------------------------------------------------------------------ */

export interface DeployAppOperationsService {
  /** 上传代码 — local `.zip` → Drive → artifact (→ optional release/deployment). */
  uploadCodeFromArchive(input: DeployUploadCodeFromArchiveInput): Promise<DeployUploadCodeResult>
  /** 上传代码 — bind a Git repository as the code source. */
  connectGitSource(appId: string, input: DeployGitSourceInput): Promise<SourceRepositoryResponse>
  /**
   * 上传代码 — 以关键字/空间搜索 Drive 节点。
   *
   * ⚠️ **当前 UI 已不再消费这个方法**：`UploadSourceDialog` 的「从 Drive
   * 选择」分支改用 `DriveNodePickerDialog`（真正的目录树浏览），因为「列出
   * 命中 `*.zip` 的扁平清单」既进不去子目录、也无法选目录。此方法保留为已发布
   * 的 service API（`publish.ts` 对外导出），供需要「按关键字跨空间搜包」的
   * 调用方使用；新代码请优先用目录树浏览。
   */
  listDriveArchives(params?: { spaceId?: string | undefined; keyword?: string | undefined }): Promise<DeployDriveArchiveOption[]>
  /** Compute the SHA-256 the artifact registration requires, in the browser. */
  archiveChecksum(file: DriveUploaderBlobLike): Promise<string>

  /** 域名设置 — read the app's publishing-domain configuration and hostnames. */
  loadDomainState(appId: string): Promise<DeployAppDomainState>
  /** 域名设置 — set/clear the `<appId>` label and the suffix catalog. */
  saveDomainConfig(appId: string, input: {
    /** `undefined` leaves the stored override alone; `null` clears it. */
    appDomainLabel?: string | null | undefined
    appDomainSuffixes?: string[] | null | undefined
  }): Promise<AppResponse>
  /** 域名设置 — register a custom hostname and bind it to the app. */
  bindCustomHostname(appId: string, input: DeployCustomHostnameInput): Promise<DeployCustomHostnameResult>
  /**
   * 域名设置 — replace the app's served bindings wholesale.
   *
   * `app.composition.update` has replace semantics, so unbinding one hostname
   * means re-sending all the others. The caller passes the complete desired set
   * and the app `version` it read, which the endpoint uses as `If-Match`.
   */
  replaceDomainBindings(appId: string, input: DeployReplaceBindingsInput): Promise<AppCompositionResponse>
  /** 域名设置 — domain zones the operator may register a hostname under. */
  listDomainZones(): Promise<readonly DomainZoneResponse[]>

  /** 详情 — everything the detail drawer shows. */
  loadAppDetail(appId: string): Promise<DeployAppDetail>
}

export interface DeployAppOperationsServiceOptions {
  readonly deployClient: SdkworkDeployAppClient
  readonly driveClient: SdkworkDriveAppClient
  readonly createIdempotencyKey?: (() => string) | undefined
  /** Injected for tests; defaults to the composed application publisher. */
  readonly publisher?: DeployApplicationPublisher | undefined
}

/** Sizes the composed Drive uploader would otherwise pick; kept explicit so the
 * progress bar and the part count stay predictable for large archives. */
const ARCHIVE_CHUNK_SIZE_BYTES = 8 * 1024 * 1024;

/** Drive archives are looked up across every space the caller can see when no
 * space is pinned, so a package uploaded elsewhere is still reusable. */
const DEFAULT_ARCHIVE_QUERY = "*.zip";

export function createDeployAppOperationsService(
  options: DeployAppOperationsServiceOptions,
): DeployAppOperationsService {
  const createIdempotencyKey = options.createIdempotencyKey ?? (() => uuid())
  const { deployClient, driveClient } = options
  // Reuse the composed publisher: it owns the chunked upload, the artifact
  // registration, and the abort/progress contract. Building a second uploader
  // here would be a second set of edge cases to keep in step.
  const publisher = options.publisher ?? createDeployApplicationPublisher({
    deployClient: {
      app: deployClient.app,
      artifact: deployClient.artifact,
      release: deployClient.release,
      deployment: deployClient.deployment,
    },
    driveClient: { uploader: driveClient.uploader },
    createIdempotencyKey,
  })

  return {
    async archiveChecksum(file) {
      const bytes = await readAllBytes(file)
      const digest = await crypto.subtle.digest("SHA-256", bytes)
      return [...new Uint8Array(digest)]
        .map((byte) => byte.toString(16).padStart(2, "0"))
        .join("")
    },

    async uploadCodeFromArchive({ appId, packageType, archive, signal, onProgress }) {
      const result = await publisher.publish({
        app: { kind: "existing", appId },
        artifact: {
          file: archive.file,
          packageType,
          fileName: archive.fileName,
          contentType: archive.contentType,
          checksumSha256: archive.checksumSha256,
          chunkSizeBytes: ARCHIVE_CHUNK_SIZE_BYTES,
          scene: "deployment-package",
          source: "@sdkwork/deployments-pc-console-publishing",
        },
        idempotencyKeys: {
          artifact: createIdempotencyKey(),
          release: createIdempotencyKey(),
          deployment: createIdempotencyKey(),
        },
        ...(signal !== undefined ? { signal } : {}),
        ...(onProgress !== undefined
          ? { onProgress: (progress) => { onProgress(normalizeUploadProgress(progress)) } }
          : {}),
      })
      return {
        artifactId: result.artifact.id,
        artifact: result.artifact.value,
        result,
      }
    },

    async connectGitSource(appId, input) {
      const repoKey = input.repoKey.trim()
      const repoUrl = input.repoUrl.trim()
      const defaultBranch = input.defaultBranch?.trim()
      const credentialSecretRef = input.credentialSecretRef?.trim()
      const idempotencyKey = createIdempotencyKey()
      return deployClient.app.sourceRepositories.create(
        appId,
        {
          repoKey,
          repoProvider: input.repoProvider,
          repoUrl,
          // Generated request types keep optional members as `?: T`, which
          // `exactOptionalPropertyTypes` will not satisfy with `T | undefined`;
          // unset members are omitted instead (the wire treats both identically).
          ...(defaultBranch === undefined || defaultBranch === "" ? {} : { defaultBranch }),
          ...(input.cloneMode === undefined ? {} : { cloneMode: input.cloneMode }),
          ...(credentialSecretRef === undefined || credentialSecretRef === ""
            ? {}
            : { credentialSecretRef }),
          idempotencyKey,
        },
        { idempotencyKey },
      )
    },

    async listDriveArchives(params) {
      const keyword = params?.keyword?.trim()
      const response = await driveClient.drive.search.list({
        q: keyword === undefined || keyword === "" ? DEFAULT_ARCHIVE_QUERY : keyword,
        ...(params?.spaceId === undefined ? {} : { spaceId: params.spaceId }),
        pageSize: "50",
      })
      return response.items
        .filter((node) => node.nodeType === "file")
        .map((node) => ({
          nodeId: node.id,
          spaceId: node.spaceId,
          fileName: node.nodeName,
          contentType: node.contentType ?? "application/zip",
          contentLength: Number(node.contentLength ?? "0"),
          updatedAt: node.updatedAt,
        }))
    },

    async loadDomainState(appId) {
      const app = await deployClient.app.retrieve(appId)
      const domains = await listAppDomains(deployClient, appId)
      return {
        app,
        domains,
        provisioned: domains.some((domain) => domain.kind === "DEFAULT"),
      }
    },

    async saveDomainConfig(appId, input) {
      // `null` clears the override and `undefined` leaves it untouched. The
      // generated request type already declares `string | null`, so the two
      // states survive to the wire unchanged.
      const body: Parameters<SdkworkDeployAppClient["app"]["update"]>[1] = {}
      if (input.appDomainLabel !== undefined) body.appDomainLabel = input.appDomainLabel
      if (input.appDomainSuffixes !== undefined) body.appDomainSuffixes = input.appDomainSuffixes
      const app = await deployClient.app.update(appId, body)
      // Saving a new label or catalog re-provisions the hostnames server-side
      // (`update_app` reconciles all environments), so the caller should reload
      // the domain list rather than patch it locally.
      return app
    },

    async listDomainZones() {
      const response = await deployClient.domain.domainZones.list({ pageSize: 100 })
      return response.items.filter((zone) => zone.status === "ACTIVE")
    },

    async bindCustomHostname(appId, input) {
      // 1. The app's current bindings, so the replacement composition does not
      //    drop hostnames the operator set up before this one.
      const existing = await listAppDomains(deployClient, appId)
      const appBefore = await deployClient.app.retrieve(appId)

      // 1b. Resolve the zone when the caller only supplied a hostname. The
      //     claim endpoint refuses names outside the zone they are claimed
      //     under, so an unresolved zone must fail here with an actionable
      //     message rather than as an opaque server error.
      let zoneId = input.zoneId ?? ""
      if (zoneId === "") {
        const zones = await deployClient.domain.domainZones.list({ pageSize: 100 }).catch(() => undefined)
        const active = (zones?.items ?? []).filter((zone) => zone.status === "ACTIVE")
        const matched = inferDomainZone(input.hostname, active)
        if (matched === undefined) {
          throw new Error(
            `No registered domain zone owns ${normalizeHostname(input.hostname)}. ` +
              "Register the domain under Domain management first.",
          )
        }
        zoneId = matched.id
      }

      // 2. Prove ownership. `EnsureDomainHostnameClaimsRequest` refuses names
      //    outside the zone and refuses duplicates *before* writing, so a failed
      //    request never leaves the tenant owning a hostname nobody asked for.
      //    A successful call also creates the `deploy_domain` row, which is why
      //    no separate hostname create follows.
      const claims = await deployClient.domain.domainZones.hostnameClaims.ensure(
        zoneId,
        { hostnames: [input.hostname] },
        { idempotencyKey: createIdempotencyKey() },
      )
      const claim = claims.items?.[0]
      if (claim === undefined) {
        throw new Error(`Domain zone ${zoneId} did not return a hostname claim.`)
      }
      const hostname = claim.hostname

      // 3. Bind it. This is the step that makes the hostname actually serve the
      //    app; without it the DNS record verifies and nothing routes.
      const bindings = [
        ...existing
          .filter((domain) => domain.domainId !== undefined)
          .map((domain) => ({
            key: compositionKey(domain.hostname, domain.pathPrefix ?? "/"),
            domainId: domain.domainId as string,
            pathPrefix: domain.pathPrefix ?? "/",
            action: { type: "SERVE" as const },
          })),
        {
          key: compositionKey(input.hostname, "/"),
          domainId: hostname.id,
          pathPrefix: "/",
          action: { type: "SERVE" as const },
        },
      ]
      const composition = await deployClient.app.composition.update(
        appId,
        {
          environment: "production",
          defaultVariantKey: "default",
          resources: [],
          variants: [{ key: "default", label: "Default" }],
          mounts: [],
          bindings,
        },
        { ifMatch: appBefore.version, idempotencyKey: createIdempotencyKey() },
      )
      return { hostname, composition }
    },

    async replaceDomainBindings(appId, input) {
      // Replace semantics: whatever this list contains is what the app serves
      // afterwards. The `If-Match` version is what makes a concurrent edit fail
      // loudly instead of silently reverting the other operator's change.
      return deployClient.app.composition.update(
        appId,
        {
          environment: "production",
          defaultVariantKey: "default",
          resources: [],
          variants: [{ key: "default", label: "Default" }],
          mounts: [],
          bindings: input.bindings.map((binding) => ({
            key: binding.key,
            domainId: binding.domainId,
            pathPrefix: binding.pathPrefix,
            action: binding.action,
          })),
        },
        { ifMatch: input.version, idempotencyKey: createIdempotencyKey() },
      )
    },

    async loadAppDetail(appId) {
      // Four independent reads: fan them out so the drawer opens in one round
      // trip rather than four sequential ones. Each is individually tolerant —
      // a tenant without any source repository should still see the app.
      const [app, domains, targets, repositories] = await Promise.all([
        deployClient.app.retrieve(appId),
        listAppDomains(deployClient, appId),
        deployClient.app.platformTargets.list(appId).then((page) => page.items).catch(() => []),
        deployClient.app.sourceRepositories.list(appId).then((page) => page.items).catch(() => []),
      ])
      return {
        app,
        domains,
        platformTargets: targets,
        sourceRepositories: repositories,
      }
    },
  }
}

/* ------------------------------------------------------------------ *
 * Helpers
 * ------------------------------------------------------------------ */

/**
 * `apps.domains.list` is a non-paginated resource list: the contract declares
 * no `page`/`pageSize` and the app has at most a few dozen hostnames, so the
 * generated client returns the items directly.
 */
async function listAppDomains(
  deployClient: SdkworkDeployAppClient,
  appId: string,
): Promise<readonly AppDomainResponse[]> {
  const response = await deployClient.app.domains.list(appId)
  return response.items ?? []
}

/**
 * The hostname's label inside its zone, which is what
 * `CreateDomainHostnameRequest.relativeName` expects (`"@"` for the apex).
 */
export function relativeNameInZone(hostname: string, apexHostname: string): string {
  const host = hostname.trim().toLowerCase().replace(/\.$/, "")
  const apex = apexHostname.trim().toLowerCase().replace(/\.$/, "")
  if (host === apex) return "@"
  const suffix = `.${apex}`
  return host.endsWith(suffix) ? host.slice(0, -suffix.length) : host
}

/**
 * A composition binding key. The server treats `key` as the identity of a
 * binding within the composition, so it must be stable across saves for the
 * same route (hostname + path prefix) or the reconcile step sees a different
 * binding every time.
 */
export function compositionKey(hostname: string, pathPrefix: string): string {
  const host = hostname.trim().toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "")
  const path = pathPrefix.trim().replace(/^\/|\/$/g, "").replace(/[^a-zA-Z0-9]+/g, "-")
  return path === "" ? host : `${host}--${path}`
}

/** Normalize a hostname the way DNS does, so matching is case/dot insensitive. */
export function normalizeHostname(value: string): string {
  return value.trim().toLowerCase().replace(/\.$/, "")
}

/**
 * Pick the zone that owns a hostname, so the operator can type a bare domain
 * instead of first choosing a zone.
 *
 * The server refuses a hostname outside the zone it is claimed under, so this
 * must choose the **longest** matching apex: with both `example.com` and
 * `eu.example.com` registered, `app.eu.example.com` belongs to the latter, and
 * picking the former would claim the wrong relative name.
 *
 * Returns `undefined` when the hostname is the apex of no registered zone. The
 * caller decides whether that is fatal — the server is still the authority, so
 * a hostname the operator owns elsewhere may legitimately be accepted later.
 */
export function inferDomainZone(
  hostname: string,
  zones: readonly DomainZoneResponse[],
): DomainZoneResponse | undefined {
  const host = normalizeHostname(hostname)
  if (host === "") return undefined
  let best: DomainZoneResponse | undefined
  for (const zone of zones) {
    const apex = normalizeHostname(zone.apexHostname)
    if (apex === "") continue
    if (host !== apex && !host.endsWith(`.${apex}`)) continue
    if (best === undefined || apex.length > normalizeHostname(best.apexHostname).length) {
      best = zone
    }
  }
  return best
}

/** A hostname is a lowercase dotted name whose labels are DNS-legal. */
export function isHostnameShaped(value: string): boolean {
  return /^[a-z0-9]([a-z0-9-]*[a-z0-9])?(\.[a-z0-9]([a-z0-9-]*[a-z0-9])?)+$/.test(
    normalizeHostname(value),
  )
}

/**
 * How many custom domains one application may serve.
 *
 * A product rule, not a mirror of a server-side cap — the server does not impose
 * one yet. It lives here rather than in the dialog so the arithmetic can be
 * asserted without a DOM.
 */
export const MAX_CUSTOM_DOMAINS = 5

export interface CustomDomainCapacity {
  /** 已生效 + 待添加的合计，用来算剩余额度。 */
  readonly used: number
  /** 还剩几个位置；到上限时为 0，不会为负。 */
  readonly remaining: number
  /** 是否可以再添加一行。 */
  readonly atCapacity: boolean
}

/**
 * Derive the custom-domain budget from how many are already bound and how many
 * drafts are open.
 *
 * `used` counts **both**, so an operator cannot open six drafts and only
 * discover the limit when binding. `remaining` is clamped at 0 so the counter
 * never renders a negative number once the cap is passed.
 */
export function customDomainCapacity(
  boundCount: number,
  draftCount: number,
  max: number = MAX_CUSTOM_DOMAINS,
): CustomDomainCapacity {
  const used = Math.max(0, boundCount) + Math.max(0, draftCount)
  const remaining = Math.max(0, max - used)
  return { used, remaining, atCapacity: remaining <= 0 }
}


/** Flatten the publisher's three-variant progress union into one shape. */
function normalizeUploadProgress(progress: ApplicationPublishProgress): DeployUploadProgress {
  switch (progress.kind) {
    case "upload":
      return {
        stage: progress.stage,
        uploading: progress.status !== "completed",
        uploadedBytes: progress.uploadedBytes,
        totalBytes: progress.totalBytes,
        uploadedParts: progress.uploadedPartsCount,
        totalParts: progress.totalParts,
      }
    case "stage":
      return {
        stage: progress.stage,
        uploading: progress.status === "started",
        uploadedBytes: 0,
        totalBytes: 0,
        uploadedParts: 0,
        totalParts: 0,
      }
    case "failure":
      return {
        stage: progress.stage,
        uploading: false,
        uploadedBytes: 0,
        totalBytes: 0,
        uploadedParts: 0,
        totalParts: 0,
        errorMessage: progress.error.message,
      }
  }
}

/**
 * Read a blob-like in full.
 *
 * `DriveUploaderBlobLike` is deliberately narrower than `Blob` — a host may
 * substitute a file-system handle — so the digest is computed from the same
 * `arrayBuffer()` accessor the uploader itself uses, and the whole archive is
 * hashed before a single byte leaves the browser. That ordering is what makes
 * the checksum an integrity claim rather than a server-side echo.
 */
async function readAllBytes(file: DriveUploaderBlobLike): Promise<ArrayBuffer> {
  if (file.arrayBuffer === undefined) {
    throw new Error("The selected archive cannot be read in this browser.")
  }
  return file.arrayBuffer()
}

/**
 * The three status axes a `DeployAppDomain` carries, mapped to message keys.
 *
 * A domain row shows three independent states and each one is an enum straight
 * off the wire. Rendering any of them raw puts an English SCREAMING_CASE token
 * next to localized siblings — `DNS: 无需验证` beside `绑定: ACTIVE` — which reads
 * as a missed translation rather than a deliberate passthrough.
 *
 * They live here, not in a component, because two surfaces (the domain dialog
 * and the detail drawer) both render a domain row; a second copy would drift.
 */
const DOMAIN_STATUS_AXIS_KEYS = {
  environment: {
    development: "domainEnvDevelopment",
    test: "domainEnvTest",
    staging: "domainEnvStaging",
    demo: "domainEnvDemo",
    production: "domainEnvProduction",
  },
  binding: {
    ACTIVE: "domainBindingActive",
    VERIFIED: "domainBindingVerified",
    PENDING: "domainBindingPending",
    PAUSED: "domainBindingPaused",
    FAILED: "domainBindingFailed",
    ARCHIVED: "domainBindingArchived",
  },
  verification: {
    NOT_REQUIRED: "domainVerificationNotRequired",
    PENDING: "domainVerificationPending",
    VERIFIED: "domainVerificationVerified",
    FAILED: "domainVerificationFailed",
    EXPIRED: "domainVerificationExpired",
  },
} as const

export type DomainStatusAxis = keyof typeof DOMAIN_STATUS_AXIS_KEYS

/** Every key the three axis tables can emit, so the caller's `t` stays typed. */
export type DomainStatusMessageKey = {
  [Axis in DomainStatusAxis]: (typeof DOMAIN_STATUS_AXIS_KEYS)[Axis][keyof (typeof DOMAIN_STATUS_AXIS_KEYS)[Axis]]
}[DomainStatusAxis]

/**
 * Resolve one of the three domain status axes to its already-translated string.
 *
 * `translate` is injected rather than imported so this module stays free of the
 * i18n catalogue (it is the UI-framework-free service layer). Unknown values
 * fall through as-is: a new server enum should surface, not disappear.
 */
export function domainStatusLabel(
  axis: DomainStatusAxis,
  value: string,
  translate: (key: DomainStatusMessageKey) => string,
): string {
  const table: Record<string, string> =
    axis === "environment"
      ? DOMAIN_STATUS_AXIS_KEYS.environment
      : axis === "binding"
        ? DOMAIN_STATUS_AXIS_KEYS.binding
        : DOMAIN_STATUS_AXIS_KEYS.verification
  const key = axis === "environment" ? table[value.toLowerCase()] : table[value.toUpperCase()]
  return key === undefined ? value : translate(key as DomainStatusMessageKey)
}

export type {
  AppCompositionResponse,
  AppKind,
  AppResponse,
  AppStatus,
  ArtifactResponse,
  CreateArtifactRequest,
  DomainHostnameResponse,
  DomainZoneResponse,
  DriveUploaderProgress,
  PageInfo,
  PlatformTargetResponse,
  SourceRepositoryResponse,
}
