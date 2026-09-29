/**
 * 发布历史：把三段读结果合并成一条时间线，并判定「这一版还能不能回滚」。
 *
 * 三次读各自独立、各自可分页，所以**合并与判定都得是纯函数** —— 它们才是这个能力
 * 真正的领域逻辑（哪一版在线上、哪一版可回滚、回滚要抄哪一次部署的参数），而
 * 客户端只负责把三页数据搬回来。写成纯函数还有一个直接好处：可以用变异测试证明
 * 门禁真的在拦人，而不是只证明「渲染不崩」。
 *
 * ## 三段关系
 *
 * ```
 *   deploy_package ──┐
 *                    ├─ packageId ─→ deploy_release ──┐
 *   deploy_release ──┘                                 ├─ releaseId ─→ deploy_deployment
 *   deploy_deployment ─────────────────────────────────┘
 * ```
 *
 * 契约上 `AppReleaseResponse.packageId` 与 `AppDeploymentResponse.releaseId` 都是
 * **可空**的（生成类型里是 `?`），所以合并必须容忍两种「挂不上」：release 找不到
 * package（分页窗口外，或制品已被清）、deployment 找不到 release（历史遗留）。
 * 两者都不许静默吞掉 —— 前者降级成「制品未知」，后者进 `orphanDeployments`
 * 单独列出。把无主部署塞进某一版下面，等于替服务端编了一段它没说过的话。
 *
 * ## 回滚
 *
 * 契约里**没有**回滚端点：`CreateAppDeploymentRequest` 是
 * `additionalProperties: false` 且不含 `rollbackFromDeploymentId`（该字段只出现在
 * 响应与 Rust repository 侧）。所以「回滚」在协议上的正解就是**以历史 release 再发
 * 一次 `deployments.create`**，而 `platformTargetId` / `deploymentKind` /
 * `deploymentTarget` / `environment` 这四元组客户端无法凭空构造 —— 它必须从既有
 * 部署记录里抄。{@link rollbackAvailability} 的存在就是为了让「抄不到」这件事
 * 有名字、有文案，而不是发出一个注定被拒的请求。
 */
import type {
  AppDeploymentResponse,
  AppReleaseResponse,
  DeploymentKind,
  DeploymentStatus,
  DeploymentTarget,
  PackageResponse,
  PackageStatus,
  ReleaseStatus,
} from "@sdkwork/deployments-pc-console-core/sdk";

/**
 * 一次历史读的页大小。
 *
 * 三条列表端点都接受 `pageSize`，且 `PageInfo` 允许到 200。历史是**只读审计面**，
 * 一次取满比翻页更符合"打开就能看到全貌"；超出这一屏的发布量属于异常，由
 * {@link DeployAppReleaseHistory.truncated} 明说，而不是悄悄只显示前 N 条。
 */
export const RELEASE_HISTORY_PAGE_SIZE = 200;

/** 详情读里可以单独失败、因而要单独说明的段落。 */
export type DeployAppHistorySection = "packages" | "releases" | "deployments";

/* ------------------------------------------------------------------ *
 * 状态词表
 * ------------------------------------------------------------------ */

/**
 * 「正在提供流量」的部署状态。
 *
 * `LIVE` 与 `ACTIVE` 是两个都在用的在线态（历史数据里两种都出现过），`DEGRADED`
 * 也算在线 —— 它说的是"服务着但健康度下降"，把它排除会让一个正在降级服务的版本
 * 既不显示为当前版本、又变成"可回滚"，而回滚到它正是此刻最不该点的按钮。
 */
export const SERVING_DEPLOYMENT_STATUSES: readonly DeploymentStatus[] = ["LIVE", "ACTIVE", "DEGRADED"];

/**
 * 「已受理但还没落地」的部署状态。
 *
 * 它拦住回滚的理由不是权限，是**时序**：一次在途部署会把线上切到它自己那一版，
 * 此时再发一次回滚，两次部署的最终归属取决于服务端执行顺序，操作员看到的结果
 * 与他的意图可能相反。所以有在途部署时不给回滚入口，先把在途的看清。
 */
export const IN_FLIGHT_DEPLOYMENT_STATUSES: readonly DeploymentStatus[] = [
  "PENDING",
  "SUBMITTING",
  "PENDING_REVIEW",
  "IN_REVIEW",
  "APPROVED",
];

/**
 * 还能拿来部署的 release 状态。
 *
 * `DRAFT` 是"还没切出来"，`RETIRED` / `ARCHIVED` 是"被主动下架"——后者尤其不能
 * 回滚：退役是一次明确的决定，再部署等于绕过它。`SUPERSEDED` / `DEPRECATED`
 * 正好是回滚最典型的来源，必须在册。
 */
export const DEPLOYABLE_RELEASE_STATUSES: readonly ReleaseStatus[] = ["ACTIVE", "SUPERSEDED", "DEPRECATED"];

/** 制品包已下架的状态：它引用的 release 即使状态可部署，也拿不出字节来。 */
export const RETIRED_PACKAGE_STATUSES: readonly PackageStatus[] = ["RETIRED", "ARCHIVED"];

/* ------------------------------------------------------------------ *
 * 合并
 * ------------------------------------------------------------------ */

/** 一条发布记录：一个不可变 release，加上它引用的制品与它派生的全部部署。 */
export interface DeployAppReleaseHistoryEntry {
  readonly release: AppReleaseResponse
  /**
   * release 引用的制品包。
   *
   * `undefined` 表示**没找到**（分页窗口外、或制品已被清理），不表示"没有" ——
   * 契约上 `packageId` 是必填，所以"没有制品"这个状态在协议里不存在。
   */
  readonly package?: PackageResponse
  /** 该 release 的部署记录，最新在前。 */
  readonly deployments: readonly AppDeploymentResponse[]
  /** 该 release 下仍在提供流量的那次部署（多次时取最新）。 */
  readonly servingDeployment?: AppDeploymentResponse
  /** 该 release 下还在途、尚未落地的部署，最新在前。 */
  readonly inFlightDeployments: readonly AppDeploymentResponse[]
}

export interface DeployAppReleaseHistoryInput {
  readonly packages: readonly PackageResponse[]
  readonly releases: readonly AppReleaseResponse[]
  readonly deployments: readonly AppDeploymentResponse[]
}

export interface DeployAppReleaseHistory {
  /** 版本时间线，最新在前。 */
  readonly entries: readonly DeployAppReleaseHistoryEntry[]
  /**
   * 当前线上版本 —— 有在线部署的那一版 release 的 id。
   *
   * 取不到时缺省（没有过任何部署，或所有部署都已失败/取消）。它同时是
   * 「这一版不用回滚，它就是线上那版」的判据。
   */
  readonly currentReleaseId?: string
  /** 当前线上那次部署本身（回滚要抄的参数就来自这里）。 */
  readonly servingDeployment?: AppDeploymentResponse
  /**
   * 与任何 release 都对不上号的部署。
   *
   * 不合并进任何一条，也不丢弃：它们证明了这台应用部署过，只是那条 release 已经
   * 读不到（例如被归档或超出分页）。丢掉它们，界面会把"部署过但记录不全"说成
   * "从没部署过"。
   */
  readonly orphanDeployments: readonly AppDeploymentResponse[]
  /** 某一段读被截断（行数超过 {@link RELEASE_HISTORY_PAGE_SIZE}）时为真。 */
  readonly truncated: boolean
}

/** int64 以字符串过线，比较必须走 BigInt —— `Number()` 在 2^53 之后静默丢精度。 */
function compareInt64String(left: string, right: string): number {
  let a: bigint
  let b: bigint
  try {
    a = BigInt(left)
    b = BigInt(right)
  } catch {
    // 非数值的 buildNumber 不该出现，但真出现时退回字典序而不是抛错：历史视图是
    // 观察面，排序不尽如人意比整屏报错好。
    return left.localeCompare(right)
  }
  return a < b ? -1 : a > b ? 1 : 0
}

/** 时间戳倒序比较；无法解析的值排到最后，而不是被当成 0（1970）排到最前。 */
function compareCreatedAtDesc(left: string, right: string): number {
  const a = Date.parse(left)
  const b = Date.parse(right)
  if (Number.isNaN(a) && Number.isNaN(b)) return 0
  if (Number.isNaN(a)) return 1
  if (Number.isNaN(b)) return -1
  return b - a
}

function isServing(status: DeploymentStatus): boolean {
  return SERVING_DEPLOYMENT_STATUSES.includes(status)
}

function isInFlight(status: DeploymentStatus): boolean {
  return IN_FLIGHT_DEPLOYMENT_STATUSES.includes(status)
}

/**
 * 合并三段读结果。
 *
 * 排序是 **`createdAt` 倒序 → buildNumber 倒序**。加第二判据不是为了好看：同一批
 * 上传常常落在同一毫秒里，只按 `createdAt` 排会让两次渲染的次序不稳定，而"上一次
 * 发布是第几版"正是这个视图唯一要回答的问题。
 */
export function mergeReleaseHistory(input: DeployAppReleaseHistoryInput): DeployAppReleaseHistory {
  const packageById = new Map(input.packages.map((entry) => [entry.id, entry]))
  const deploymentsByRelease = new Map<string, AppDeploymentResponse[]>()
  const orphanDeployments: AppDeploymentResponse[] = []

  for (const deployment of input.deployments) {
    const releaseId = deployment.releaseId
    if (releaseId === undefined || releaseId === "") {
      orphanDeployments.push(deployment)
      continue
    }
    const bucket = deploymentsByRelease.get(releaseId)
    if (bucket === undefined) {
      deploymentsByRelease.set(releaseId, [deployment])
    } else {
      bucket.push(deployment)
    }
  }

  // 关联不上的部署可能指向一个**读不到的** release（超出分页、或不属于本应用）。
  // 只有真在 releases 里出现过才不算孤儿 —— 判据是这一批读回来的集合本身。
  const knownReleaseIds = new Set(input.releases.map((release) => release.id))
  for (const [releaseId, bucket] of deploymentsByRelease) {
    if (knownReleaseIds.has(releaseId)) continue
    orphanDeployments.push(...bucket)
    deploymentsByRelease.delete(releaseId)
  }

  const sortedReleases = [...input.releases].sort((left, right) => {
    const byTime = compareCreatedAtDesc(left.createdAt, right.createdAt)
    if (byTime !== 0) return byTime
    return compareInt64String(right.buildNumber, left.buildNumber)
  })

  let currentReleaseId: string | undefined
  let servingDeployment: AppDeploymentResponse | undefined

  const entries = sortedReleases.map((release): DeployAppReleaseHistoryEntry => {
    const deployments = [...(deploymentsByRelease.get(release.id) ?? [])]
      .sort((left, right) => compareCreatedAtDesc(left.createdAt, right.createdAt))
    const serving = deployments.filter((deployment) => isServing(deployment.deploymentStatus))
    const inFlight = deployments.filter((deployment) => isInFlight(deployment.deploymentStatus))
    // 线上版本取**最新**的在线部署：同一个 release 可以被部署多次（例如回滚之后再
    // 正向部署），真正在服务的是最近那次。
    const servingForRelease = serving[0]
    if (servingForRelease !== undefined) {
      if (servingDeployment === undefined
        || compareCreatedAtDesc(servingDeployment.createdAt, servingForRelease.createdAt) > 0) {
        servingDeployment = servingForRelease
        currentReleaseId = release.id
      }
    }
    const resolvedPackage = packageById.get(release.packageId)
    return {
      release,
      ...(resolvedPackage === undefined ? {} : { package: resolvedPackage }),
      deployments,
      ...(servingForRelease === undefined ? {} : { servingDeployment: servingForRelease }),
      inFlightDeployments: inFlight,
    }
  })

  orphanDeployments.sort((left, right) => compareCreatedAtDesc(left.createdAt, right.createdAt))

  return {
    entries,
    ...(currentReleaseId === undefined ? {} : { currentReleaseId }),
    ...(servingDeployment === undefined ? {} : { servingDeployment }),
    orphanDeployments,
    truncated:
      input.packages.length >= RELEASE_HISTORY_PAGE_SIZE
      || input.releases.length >= RELEASE_HISTORY_PAGE_SIZE
      || input.deployments.length >= RELEASE_HISTORY_PAGE_SIZE,
  }
}

/* ------------------------------------------------------------------ *
 * 回滚判定
 * ------------------------------------------------------------------ */

/** 回滚要抄的四元组：它无法从 release 推出，只能从既有部署记录里拿。 */
export interface DeployAppRollbackPlan {
  readonly releaseId: string
  readonly platformTargetId: string
  readonly deploymentKind: DeploymentKind
  readonly deploymentTarget: DeploymentTarget
  readonly environment: string
  /** 参数抄自哪一次部署 —— 界面要能说清"按哪次的配置回滚"。 */
  readonly referenceDeploymentId: string
  /** 那次部署是不是当前线上那次（否则是抄了同一 release 的更早一次）。 */
  readonly referenceIsServing: boolean
}

/**
 * 不能回滚的理由。每一个都对应一条独立文案 —— 把五种原因压成一句
 * 「当前不可回滚」等于让操作员去猜，而其中三种（在途、退役、无参照）各自
 * 需要完全不同的下一步动作。
 */
export type DeployAppRollbackBlockReason =
  | "serving"
  | "inFlight"
  | "releaseNotDeployable"
  | "packageNotDeployable"
  | "noReferenceDeployment"

export type DeployAppRollbackAvailability =
  | { readonly kind: "available"; readonly plan: DeployAppRollbackPlan }
  | { readonly kind: "unavailable"; readonly reason: DeployAppRollbackBlockReason }

/** 一次部署是否四条参数齐全 —— 缺一条就抄不出可执行的请求。 */
function deploymentIsComplete(
  deployment: AppDeploymentResponse,
): deployment is AppDeploymentResponse & {
  platformTargetId: string
  deploymentKind: DeploymentKind
  deploymentTarget: DeploymentTarget
} {
  return deployment.platformTargetId !== undefined
    && deployment.deploymentKind !== undefined
    && deployment.deploymentTarget !== undefined
}

/**
 * 这一版能不能回滚；能的话把请求参数算出来。
 *
 * 判定顺序刻意从"最容易理解"排到"最技术"：先排除"它就是在线上那一版"（没什么可
 * 回滚的）、再排除"有在途部署"（时序问题），然后才是 release / 制品的状态，最后
 * 才是"抄不到参照部署"这种实现层面的原因。
 *
 * 参照部署的优先级是 **当前线上那次 → 这一版自己最新的完整部署**。先取线上是因为
 * 回滚是"把线上换回去"，要复现的是**线上那次的环境与目标**；只有当整个应用都没有
 * 在线部署时，才退而使用这一版自己部署过的那次。
 */
export function rollbackAvailability(
  entry: DeployAppReleaseHistoryEntry,
  history: DeployAppReleaseHistory,
): DeployAppRollbackAvailability {
  if (history.currentReleaseId === entry.release.id) {
    return { kind: "unavailable", reason: "serving" }
  }
  if (entry.inFlightDeployments.length > 0) {
    return { kind: "unavailable", reason: "inFlight" }
  }
  if (!DEPLOYABLE_RELEASE_STATUSES.includes(entry.release.releaseStatus)) {
    return { kind: "unavailable", reason: "releaseNotDeployable" }
  }
  // 制品**读不到**不阻断：`releaseStatus` 才是这次部署的服务端判据，而读不到多半是
  // 客户端分页窗口的问题。制品**读到了且已下架**才阻断 —— 那是确定的事实。
  if (entry.package !== undefined && RETIRED_PACKAGE_STATUSES.includes(entry.package.packageStatus)) {
    return { kind: "unavailable", reason: "packageNotDeployable" }
  }

  const own = entry.deployments.find(deploymentIsComplete)
  const serving = history.servingDeployment
  const reference = serving !== undefined && deploymentIsComplete(serving) ? serving : own
  if (reference === undefined) {
    return { kind: "unavailable", reason: "noReferenceDeployment" }
  }

  return {
    kind: "available",
    plan: {
      releaseId: entry.release.id,
      platformTargetId: reference.platformTargetId,
      deploymentKind: reference.deploymentKind,
      deploymentTarget: reference.deploymentTarget,
      environment: reference.environment,
      referenceDeploymentId: reference.id,
      referenceIsServing: reference.id === serving?.id,
    },
  }
}

/* ------------------------------------------------------------------ *
 * 展示辅助
 * ------------------------------------------------------------------ */

const BYTE_UNITS = ["B", "KB", "MB", "GB", "TB"] as const;

/**
 * int64 字节数 → 可读尺寸。入参是字符串（`x-sdkwork-int64-string: true`），
 * 所以先把字符串原样解析成 `number` —— 制品包不会超过 9 PB，2^53 足够。
 */
export function formatByteSize(value: string | undefined): string | undefined {
  if (value === undefined || value === "") return undefined
  const bytes = Number(value)
  if (!Number.isFinite(bytes) || bytes < 0) return undefined
  if (bytes === 0) return "0 B"
  let unit = 0
  let scaled = bytes
  while (scaled >= 1024 && unit < BYTE_UNITS.length - 1) {
    scaled /= 1024
    unit += 1
  }
  // 字节数不显示小数（"512.0 B"没有信息量），KB 以上保留一位。
  const rendered = unit === 0 ? String(Math.round(scaled)) : scaled.toFixed(scaled < 10 ? 1 : 0)
  return `${rendered} ${BYTE_UNITS[unit]}`
}

/** 摘要折半显示：校验和是 64 位十六进制，整串在表格里只会把别的列挤走。 */
export function shortDigest(value: string | undefined, keep = 12): string | undefined {
  if (value === undefined || value.length <= keep) return value
  return `${value.slice(0, keep)}…`
}

/** 时间戳 → 本地化字符串；无法解析时原样返回，不显示 "Invalid Date"。 */
export function formatHistoryTime(value: string, locale: string): string {
  const date = new Date(value)
  return Number.isNaN(date.getTime()) ? value : date.toLocaleString(locale)
}

/**
 * 状态色调。枚举 → 四档语义色，由界面映射到具体样式类。
 *
 * 它放在这里而不是组件里，是因为这是**一条判断**而不是一段样式：`DEGRADED`
 * 算「警告」而不是「失败」、`ROLLED_BACK` 算「中性」而不是「失败」—— 后者尤其
 * 容易写错，回滚是**成功执行的一次动作**，把它染红会让运维把正常操作读成事故。
 */
export type DeployStatusTone = "ok" | "warn" | "bad" | "neutral";

export function releaseStatusTone(status: ReleaseStatus): DeployStatusTone {
  switch (status) {
    case "ACTIVE":
      return "ok"
    case "DRAFT":
      return "warn"
    case "SUPERSEDED":
    case "DEPRECATED":
      return "neutral"
    case "RETIRED":
    case "ARCHIVED":
      return "bad"
  }
}

export function packageStatusTone(status: PackageStatus): DeployStatusTone {
  switch (status) {
    case "READY":
    case "VALIDATED":
      return "ok"
    case "DRAFT":
      return "warn"
    case "SUPERSEDED":
      return "neutral"
    case "RETIRED":
    case "ARCHIVED":
      return "bad"
  }
}

export function deploymentStatusTone(status: DeploymentStatus): DeployStatusTone {
  switch (status) {
    case "LIVE":
    case "ACTIVE":
      return "ok"
    case "DEGRADED":
    case "PENDING":
    case "SUBMITTING":
    case "PENDING_REVIEW":
    case "IN_REVIEW":
    case "APPROVED":
      return "warn"
    case "REJECTED":
    case "FAILED":
    case "CANCELLED":
      return "bad"
    case "ROLLED_BACK":
      return "neutral"
  }
}
