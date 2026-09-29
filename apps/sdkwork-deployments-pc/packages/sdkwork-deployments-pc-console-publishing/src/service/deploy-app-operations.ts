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
  AppClientClass,
  AppCompositionResponse,
  AppDeploymentResponse,
  AppDomainResponse,
  AppKind,
  AppPublishEnvironment,
  AppResponse,
  AppSourceSpecResponse,
  AppStatus,
  ArtifactResponse,
  BindAppSourceSpecSourceRequest,
  CreateAppDeploymentRequest,
  CreateAppSourceSpecRequest,
  CreateArtifactRequest,
  CreateSourceRepositoryRequest,
  DomainHostnameResponse,
  DomainZoneResponse,
  PageInfo,
  PlatformTargetResponse,
  SourceRepositoryResponse,
  SdkworkDeployAppClient,
  UpdateAppSourceSpecRequest,
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
import {
  mergeReleaseHistory,
  RELEASE_HISTORY_PAGE_SIZE,
  type DeployAppHistorySection,
  type DeployAppReleaseHistory,
  type DeployAppRollbackPlan,
} from "./app-release-history.ts";
import { planClientClassDefault, type SourceSpecRoutingInput } from "./app-source-spec-routing.ts";

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

/**
 * Sections of the detail drawer whose read can fail on its own.
 *
 * The drawer fans its reads out concurrently and keeps each one *tolerant* — one
 * unreachable endpoint must not blank the whole drawer. What it must not do is
 * render that failure as "there is nothing here": an operator who may not read
 * source repositories would otherwise be told the application has none. So the
 * section is named here and the drawer says "could not load" instead.
 */
export type DeployAppDetailSection = "platformTargets" | "sourceRepositories" | "sourceSpecs"

/** Everything the detail drawer renders, gathered from four endpoints. */
export interface DeployAppDetail {
  readonly app: AppResponse
  readonly domains: readonly DeployAppDomain[]
  readonly platformTargets: readonly PlatformTargetResponse[]
  readonly sourceRepositories: readonly SourceRepositoryResponse[]
  /**
   * 源码规格：这个应用有哪些源码，各自服务哪些客户端。
   *
   * 与 `sourceRepositories` 是两个不同的来源面 —— 那个是 **git 仓库**（代码从哪来），
   * 这个是**已上传的分发来源**（每次访问由哪份产物服务）。两者同名「来源」但不可合并：
   * 一个应用可以只绑了 git 仓库而没有任何可分发产物。
   */
  readonly sourceSpecs: readonly AppSourceSpecResponse[]
  /**
   * The sections above that came back empty because their read *failed*, not
   * because the application has none. Empty when every read succeeded.
   */
  readonly unavailableSections: readonly DeployAppDetailSection[]
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

  /* ---------------------------------------------------------------- *
   * 源码规格（source specs）
   * ---------------------------------------------------------------- *
   * A spec is one uploaded source of the app (a PC bundle, an H5 bundle, …)
   * plus the client classes it serves. The console owns two things here that
   * the wire does not spell out: which spec is a class's default (lowest
   * `preference`), and that a spec with no uploaded source serves nothing.
   * Both are computed by `app-source-spec-routing.ts`; these methods only carry
   * the bytes of the round trip.
   */

  /** 源码规格 — 一个应用在某环境下的全部规格（含每端路由与来源状态）。 */
  listSourceSpecs(appId: string, environment?: AppPublishEnvironment): Promise<readonly AppSourceSpecResponse[]>
  createSourceSpec(appId: string, input: CreateAppSourceSpecRequest): Promise<AppSourceSpecResponse>
  /**
   * 源码规格 — 更新一个规格。
   *
   * `version` is the optimistic-concurrency token and becomes `If-Match`: the
   * contract declares the endpoint idempotent-conditioned, so a stale write must
   * fail loudly rather than overwrite a concurrent edit. `specKey` is not in the
   * patch — it is the projected variant key, and renaming it would silently
   * re-point every routing rule that names it.
   */
  updateSourceSpec(
    appId: string,
    specId: string,
    input: UpdateAppSourceSpecRequest,
    version: string,
  ): Promise<AppSourceSpecResponse>
  deleteSourceSpec(appId: string, specId: string): Promise<void>
  /**
   * 源码规格 — 把已上传的来源登记到某个规格上。
   *
   * Separate from `updateSourceSpec` because it is a different fact: the spec
   * describes *how* a source is served, this says *which* source it is. A spec
   * is normally declared first and filled by the upload that follows, which is
   * why `sourceStatus` is its own field.
   */
  bindSourceSpecSource(
    appId: string,
    specId: string,
    source: BindAppSourceSpecSourceRequest["source"],
  ): Promise<AppSourceSpecResponse>
  /**
   * 源码规格 — 把某个客户端类别的**默认**规格改成 `targetSpecId`。
   *
   * `specs` is the set the decision was made from, and each spec's `version`
   * travels with its write, so a concurrent edit surfaces as a failed
   * precondition instead of a lost route. The sequence is planned by
   * {@link planClientClassDefault} — the outgoing default is *demoted*, not
   * dropped, so the class keeps serving while the new default is still `EMPTY`.
   */
  setClientClassDefault(
    appId: string,
    clientClass: AppClientClass,
    targetSpecId: string,
    specs: readonly (SourceSpecRoutingInput & { readonly version: string })[],
  ): Promise<void>

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

  /** 发布历史 — 制品包 / 版本 / 部署三段读结果合并成一条时间线。 */
  loadReleaseHistory(appId: string): Promise<DeployAppReleaseHistorySnapshot>
  /**
   * 发布历史 — 回滚：把历史版本重新部署一次。
   *
   * 契约里没有回滚端点（`CreateAppDeploymentRequest` 既不含
   * `rollbackFromDeploymentId`，也是 `additionalProperties: false`），所以回滚在
   * 协议上的唯一正解就是再发一次 `deployments.create`。四元组
   * `platformTargetId` / `deploymentKind` / `deploymentTarget` / `environment`
   * 由 {@link rollbackAvailability} 从既有部署记录里抄出来，本方法只负责把它
   * 送上路 —— 幂等键与头部成对，与其他写命令一致。
   */
  rollbackRelease(appId: string, plan: DeployAppRollbackPlan): Promise<AppDeploymentResponse>
}

/**
 * 发布历史的完整读结果：合并后的时间线 **加上**哪几段没读到。
 *
 * `unavailableSections` 与 {@link DeployAppDetail.unavailableSections} 是同一条
 * 判据、同一个理由：三段读各自容错，容错之后「读失败」与「本来就没有」必须还能
 * 分开说。一个制品包都没有，和一个读不到制品包的调用者，看到的应该是两句话。
 */
export interface DeployAppReleaseHistorySnapshot extends DeployAppReleaseHistory {
  readonly unavailableSections: readonly DeployAppHistorySection[]
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
      // Five independent reads: fan them out so the drawer opens in one round
      // trip rather than five sequential ones. Each is individually tolerant —
      // a tenant without any source repository should still see the app, and an
      // app whose specs have not been declared yet should still show its domains.
      //
      // Tolerance on its own is what made this misleading: swallowing the error
      // left each section rendering its *empty* state, so "we could not read this"
      // and "there is nothing here" were indistinguishable. The catch still
      // returns an empty list, but it also records which section it was.
      const unavailableSections: DeployAppDetailSection[] = []
      const tolerant = <T>(section: DeployAppDetailSection, read: Promise<readonly T[]>): Promise<readonly T[]> =>
        read.catch(() => {
          unavailableSections.push(section)
          return []
        })
      const [app, domains, targets, repositories, sourceSpecs] = await Promise.all([
        deployClient.app.retrieve(appId),
        listAppDomains(deployClient, appId),
        tolerant("platformTargets", deployClient.app.platformTargets.list(appId).then((page) => page.items)),
        tolerant("sourceRepositories", deployClient.app.sourceRepositories.list(appId).then((page) => page.items)),
        tolerant("sourceSpecs", deployClient.app.sourceSpecs.list(appId).then((page) => page.items)),
      ])
      return {
        app,
        domains,
        platformTargets: targets,
        sourceRepositories: repositories,
        sourceSpecs,
        // `Promise.all` settles only after every catch handler has run, so the
        // pushes above are all visible here.
        unavailableSections,
      }
    },

    async loadReleaseHistory(appId) {
      // 三段读并发，且**各自容错** —— 理由与详情抽屉同一套：读不到部署记录不该
      // 让整屏变成错误页，但也不能把「读失败」渲染成「从没发布过」。容错只负责
      // 给出空数组，失败的那一段记在 `unavailableSections` 里由界面说清。
      //
      // 三段都只取一页，且页大小取到契约上限（200）：历史是只读审计面，一次取满
      // 比翻页更贴合「打开就看到全貌」。超出上限时 `mergeReleaseHistory` 会把
      // `truncated` 置真，界面明说只显示了前 N 条。
      const unavailableSections: DeployAppHistorySection[] = []
      const tolerant = <T>(
        section: DeployAppHistorySection,
        read: Promise<readonly T[]>,
      ): Promise<readonly T[]> => read.catch(() => {
        unavailableSections.push(section)
        return []
      })
      const [packages, releases, deployments] = await Promise.all([
        tolerant("packages", deployClient.package.list(appId, { pageSize: RELEASE_HISTORY_PAGE_SIZE }).then((page) => page.items)),
        tolerant("releases", deployClient.release.list(appId, { pageSize: RELEASE_HISTORY_PAGE_SIZE }).then((page) => page.items)),
        tolerant("deployments", deployClient.deployment.list(appId, { pageSize: RELEASE_HISTORY_PAGE_SIZE }).then((page) => page.items)),
      ])
      return {
        ...mergeReleaseHistory({ packages, releases, deployments }),
        unavailableSections,
      }
    },

    rollbackRelease(appId, plan) {
      // 幂等键同时进 body 与 `Idempotency-Key` 头。`CreateAppDeploymentRequest`
      // 把 `idempotencyKey` 声明成 **required body** 成员（`additionalProperties:
      // false`），而 Rust 授权层对空白串直接拒绝，所以两处必须来自同一个值 ——
      // 各自新生成一次会让重试看起来像第二条命令。
      const idempotencyKey = createIdempotencyKey()
      const request: CreateAppDeploymentRequest = {
        platformTargetId: plan.platformTargetId,
        releaseId: plan.releaseId,
        deploymentKind: plan.deploymentKind,
        deploymentTarget: plan.deploymentTarget,
        environment: plan.environment,
        idempotencyKey,
      }
      return deployClient.deployment.create(appId, request, { idempotencyKey })
    },

    async listSourceSpecs(appId, environment) {
      const page = await deployClient.app.sourceSpecs.list(
        appId,
        environment === undefined ? undefined : { environment },
      )
      return page.items
    },

    createSourceSpec(appId, input) {
      // `environment` is part of the create body, not a query parameter: the
      // spec belongs to one environment and the row's uniqueness is scoped by it.
      return deployClient.app.sourceSpecs.create(appId, input, {
        idempotencyKey: createIdempotencyKey(),
      })
    },

    updateSourceSpec(appId, specId, input, version) {
      return deployClient.app.sourceSpecs.update(appId, specId, input, { ifMatch: version })
    },

    deleteSourceSpec(appId, specId) {
      return deployClient.app.sourceSpecs.delete(appId, specId)
    },

    bindSourceSpecSource(appId, specId, source) {
      return deployClient.app.sourceSpecs.bindSource(appId, specId, { source })
    },

    async setClientClassDefault(appId, clientClass, targetSpecId, specs) {
      const plan = planClientClassDefault(specs, clientClass, targetSpecId)
      // Sequential, not concurrent: the first write releases rank 0 and the
      // second claims it. Racing them can land the claim first, which the
      // database rejects with the unique index on (class, preference).
      for (const update of plan.updates) {
        await deployClient.app.sourceSpecs.update(
          appId,
          update.specId,
          {
            clientClassRoutes: update.routes.map((route) => ({
              clientClass: route.clientClass,
              preference: route.preference,
            })),
          },
          { ifMatch: update.version },
        )
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
