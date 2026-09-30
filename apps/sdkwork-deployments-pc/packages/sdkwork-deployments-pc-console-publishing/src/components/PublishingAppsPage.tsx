/**
 * The applications ledger: one page, two surfaces, and one axis between them.
 *
 *   ┌ admin ─────────────────────────────────────────────────────────────────────┐
 *   │ 归属 [ 全部 | 平台应用 | 租户应用 | 组织应用 | 个人应用 ]  ← 服务端分面   │
 *   │ [ 关键字 ]  [ 应用类型: SPA 2 · Android 1 ]  [ 清空 ]      显示 3 / 共 3  │
 *   │ ───────────────────────────────────────────────────────────────────────── │
 *   │ [◼] 名称      | 类型 | 状态 | 归属类型 | 归属用户 | 域名 | …             │
 *   │     标识副行  │                                                          │
 *   └────────────────────────────────────────────────────────────────────────────┘
 *
 *   ┌ console ───────────────────────────────────────────────────────────────────┐
 *   │ [ 关键字 ]  [ 应用类型: SPA 2 · Android 1 ]  [ 清空 ]      显示 3 / 共 3  │
 *   │ ───────────────────────────────────────────────────────────────────────── │
 *   │ [◼] 名称      | 类型 | 状态 | 归属类型 | 归属用户 | 域名 | …             │
 *   │     标识副行  │                                                          │
 *   └────────────────────────────────────────────────────────────────────────────┘
 *
 * The identity column leads with an avatar tile (`AppAvatar`): a deterministic
 * gradient + application-kind glyph always, the uploaded store icon
 * (`metadata.media.icon`, resolved to a signed URL) fading in over it when the
 * app carries one. Name and slug share that one column — the industry ledger
 * pattern — so the row is recognized by shape before it is read.
 *
 * **Every facet sits above the table, on both surfaces.** An earlier cut put the
 * ownership levels in a left rail next to an application-type panel. The rail is
 * gone for two reasons, and neither depends on which surface is rendering: it
 * spent 208px of a ten-column ledger on a control whose answer each row already
 * prints (`归属类型` / `归属用户`), and on the console it was a selector with a
 * single effective answer. What is left is one toolbar directly above the rows it
 * narrows, so the narrowing and its effect are in the same glance.
 *
 * **The ownership axis is what the two surfaces disagree about, and `surface`
 * says which one this is.** They disagree because their reach differs:
 *
 * - `admin` reaches **every** ownership level, so it gets the axis as a
 *   single-select **tab row** — and that facet is applied **by the server**: a
 *   tab is pushed down as `apps.list`'s `scope`, so turning a page can never
 *   return rows from a level the operator just left. The tab is server-side
 *   because the row set really changes; a local filter over one page is the drift
 *   {@link loadAllApps} exists to remove.
 * - `console` reaches **one** level (the caller's own apps), so it gets **no
 *   control at all** rather than a selector with a single option. This is the
 *   same rule the IAM cloud account page states as `levelIsSelectable`: a level
 *   that cannot change anything is noise, and it is withheld. The axis stays
 *   legible anyway — see the two ownership columns below.
 *
 * `surface` defaults to `"console"`, the narrower surface, so a host that is not
 * the platform admin passes nothing (the same convention
 * `CloudAccountManagementSurface` uses). It is also the only way for a `scope`
 * to reach a request: the console branch cannot set the state the request reads,
 * so the reachable set stays whatever the server's ownership gate answers.
 *
 * **The type facet is chips, and it narrows in this browser.** `apps.list` accepts
 * no type parameter, so a chip cannot be pushed down; instead the ledger loads
 * every page of the reachable set ({@link loadAllApps}) and filters it locally.
 * That is also what makes the per-chip counts exact rather than "counts of
 * whatever happened to load" — they are counted from the very rows the chips
 * filter. See `service/app-list-facets.ts` for why the facet is `app_kind`
 * (contract-complete on every row) and not the H5/PC surface (only recorded by the
 * publish flow, absent on an app that was never published).
 *
 * The lifecycle itself is unchanged: one header command (create) plus nine row
 * commands — 编辑 / 源码规格 / 修改源码 / 发布 / 发布历史 / 域名设置 / 详情 / 启停 / 归档.
 * Publishing is a per-app row action, so with an empty table there is nothing to
 * publish.
 *
 * 「发布历史」紧跟在「发布」之后，而不是并到「详情」里：它是那次命令的**审计面**
 * ——「我刚发布的是什么、上一版是什么、能不能退回去」。两者的读取面不同（详情读四
 * 个配置端点，历史读 package/release/deployment 三段），合并会让一个长页面里既有
 * 静态字段又有时间线，而运维最需要的恰是「现在」与「一路怎么来的」分开看。
 *
 * Ownership is two columns, not one. `apps.list` is tenant-wide, so without them a
 * platform-operated app, a tenant's shared app and one person's personal app are
 * indistinguishable in the ledger. `归属类型` names the level and `归属用户`
 * names the concrete owner — a user id for `USER`, an organization id for
 * `ORGANIZATION`, and the level's own name for the two tenant-wide levels that
 * have no single owner, so the level is never printed twice.
 *
 * **The domain column needs no extra request.** `AppResponse` already carries the
 * *effective* `appDomainLabel` and `appDomainSuffixes`, so the canonical
 * production hostname is derived locally with the same rule the server uses
 * (`<label>.app.<suffix>`, see `sdkwork-deploy-core::default_app_hostname`).
 *
 * Clients arrive as props (no console-core context dependency), so the same page
 * can be embedded by any host that can construct the two generated clients — the
 * deployments console shell, the Web Server console bridge and the BirdCoder
 * plugin alike. `surface` is the one piece of host identity it needs, and it
 * selects copy and one control, never an authorization decision: the server's
 * ownership gate is what decides which levels a caller may read (see the
 * `app_owner_gate` default in `app-list-facets.ts`'s sibling service), and `scope`
 * can only narrow that answer.
 *
 * 全部用户可见文案走 publishingTranslator 目录（I18N_SPEC v2.0 §1：用户可见
 * 文本必须来自 message catalog）；枚举值经 APP_KIND/APP_STATUS/APP_OWNER_TYPE
 * 映射表本地化，未覆盖的新枚举值回退原文展示。
 */
import { useEffect, useMemo, useState } from "react";
import { DataTable, type DataTableColumn } from "@sdkwork/ui-pc-react";
import type { AppKind, AppOwnerType, AppResponse, AppStatus, SdkworkDeployAppClient } from "@sdkwork/deployments-pc-console-core/sdk";
import type { SdkworkDriveAppClient } from "@sdkwork/deployments-pc-console-core/sdk";
import type { DeploymentsLocale } from "@sdkwork/deployments-pc-commons";
import { Archive, CirclePause, CirclePlay, Globe, History, Info, Layers, Pencil, Rocket, Search, Upload, X, type LucideIcon } from "lucide-react";
import {
  publishingTranslator,
  APP_KIND_LABEL_KEYS,
  APP_OWNER_SCOPE_LABEL_KEYS,
  APP_OWNER_TYPE_LABEL_KEYS,
  APP_STATUS_LABEL_KEYS,
  type PublishingMessageKey,
  type PublishingTranslator,
} from "../i18n.ts";
import { createDeployAppPublishingService } from "../service/deploy-app-publishing.ts";
import { createAppIconUrlResolver } from "../service/app-icon.ts";
import {
  APP_LIST_PAGE_SIZE,
  countAppKinds,
  filterApps,
  loadAllApps,
  normalizeKeyword,
  orderAppKindFacets,
} from "../service/app-list-facets.ts";
import { AppDetailDrawer } from "./AppDetailDrawer.tsx";
import { AppAvatar } from "./AppAvatar.tsx";
import { AppDomainDialog } from "./AppDomainDialog.tsx";
import { AppReleaseHistoryDrawer } from "./AppReleaseHistoryDrawer.tsx";
import { AppSourceSpecsDrawer } from "./AppSourceSpecsDrawer.tsx";
import { CreateAppDialog } from "./CreateAppDialog.tsx";
import { UploadSourceDialog } from "./UploadSourceDialog.tsx";
// The 「发布」 row action now opens the dialog that actually publishes. It used to
// open `CreateDeployAppDialog`, whose submit writes nothing but `deploy_app`
// metadata (version/environment/framework/category/media) via `app.update` — no
// release, no deployment — so the ledger could never show the publication the
// button claimed. That component is still live in the BirdCoder plugin and keeps
// its own surface; it is just no longer the console's publish path.
import { AppArchiveDialog, AppEditDialog, AppPublishDialog } from "./AppOperationsDialogs.tsx";
import "./create-deploy-app.module.css";

export interface PublishingAppsPageProps {
  readonly deployClient: SdkworkDeployAppClient
  readonly driveClient: SdkworkDriveAppClient
  readonly locale: DeploymentsLocale
  /**
   * Which surface is rendering the ledger. It selects the ownership facet — the
   * admin surface reaches every ownership level and gets a tab row for it, the
   * console reaches one and gets no control — and nothing else.
   *
   * Defaults to `console`, the narrower surface, so a host that is not the
   * platform admin has to pass nothing (the convention
   * `CloudAccountManagementSurface` already uses in the Web Server console).
   */
  readonly surface?: "admin" | "console"
}

const APP_TABLE_PAGE_SIZE_OPTIONS = [20, 50, 100] as const;

/**
 * 应用类型筛选项的展示顺序，取自同一张 `Record<AppKind, …>` 文案表 —— 契约新增
 * 枚举值时它必编译错，于是筛选项与文案一起补齐。顺序本身由 service 层的 rank
 * 表决定，未登记的成员排到末尾而**不会**被丢掉。
 */
const APP_KIND_FACET_ORDER: readonly AppKind[] = orderAppKindFacets(
  Object.keys(APP_KIND_LABEL_KEYS) as readonly AppKind[],
);

/**
 * 归属分面的四个层级，取自同一张 `Record<AppOwnerType, …>` 文案表 —— 理由同
 * `APP_KIND_FACET_ORDER`：契约新增层级时这里必编译错，标签与选项一起补齐。
 *
 * 顺序即契约 `AppOwnerType` 的声明序（平台 → 租户 → 组织 → 用户），也就是
 * 服务端 `AppOwnerType::rank` 的收窄序：从最广的层级读到最窄的那个，读起来
 * 就是「(全平台) → (全租户) → (全组织) → (个人)」。
 */
const APP_OWNER_SCOPE_OPTIONS: readonly AppOwnerType[] = Object.keys(
  APP_OWNER_TYPE_LABEL_KEYS,
) as readonly AppOwnerType[];

/** 行内运维命令的一条。`danger` 只改观感、不禁用 —— 不可逆动作靠确认框兜底。 */
interface RowAction {
  readonly key: PublishingMessageKey
  readonly icon: LucideIcon
  readonly onSelect: () => void
  readonly danger?: boolean
}

/** 枚举 → 本地化文案；映射表未覆盖的新枚举值回退原文。 */
function enumLabel(
  kind: AppKind | AppStatus | AppOwnerType,
  table: Readonly<Record<string, PublishingMessageKey>>,
  t: PublishingTranslator,
): string {
  const key: PublishingMessageKey | undefined = table[kind];
  return key !== undefined ? t(key) : kind;
}

/**
 * The app's canonical hostname, derived exactly as the server does.
 *
 * `appDomainLabel` / `appDomainSuffixes` on the response are the *effective*
 * values (override when set, platform catalog otherwise), so this needs no
 * request. Production uses the bare `app` label; that is the hostname an
 * operator quotes when someone asks "where is this app".
 */
export function primaryHostname(app: AppResponse): string | undefined {
  const label = app.appDomainLabel ?? app.slug
  const suffix = app.appDomainSuffixes?.[0]
  if (label === "" || suffix === undefined) return undefined
  return `${label}.app.${suffix}`
}

export function PublishingAppsPage({ deployClient, driveClient, locale, surface = "console" }: PublishingAppsPageProps) {
  const t = useMemo(() => publishingTranslator(locale), [locale])
  const service = useMemo(
    () => createDeployAppPublishingService({ deployClient, driveClient }),
    [deployClient, driveClient],
  )
  /**
   * 身份列头像的签名 URL 解析器。每页一个实例 —— 缓存与在途去重都长在闭包里，
   * 换客户端（重新挂载）才换缓存；整个台账共用一个，行复用、翻页、切分面都
   * 不会重复兑换同一枚图标的下载 URL。失败返回 `undefined`，由头像自己落回
   * 渐变底（见 service/app-icon.ts：头像缺失是展示事实，不进错误横幅）。
   */
  const resolveIconUrl = useMemo(
    () => createAppIconUrlResolver({ drive: driveClient.drive }),
    [driveClient],
  )
  /**
   * 归属分面是不是一道**真正的选择** —— 即，这个面到不到得了多于一档的归属。
   *
   * 与 IAM 云账号页的 `levelIsSelectable` 同一条规则：只有一档可选的控件不是筛选，
   * 是噪声。console 到得了的只有调用者自己的应用，所以它一档都不给；admin 到得了
   * 四个层级，于是给一排 tab。判据只有 `surface` 一个来源，因为 `apps.list` 不回传
   * 「你能读到哪些层级」——可达集是服务端闸门的事，客户端拿不到清单也就无从派生。
   */
  const isAdmin = surface === "admin"
  /** 当前归属分面下已加载的**完整**行集合（不止一页）——见 loadAllApps。 */
  const [apps, setApps] = useState<AppResponse[]>([])
  /** 服务端仍有行、但页数上限先到了：类型计数与筛选只覆盖已加载的那些行。 */
  const [truncated, setTruncated] = useState(false)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string>()
  const [notice, setNotice] = useState<string>()
  /**
   * 归属 tab 选中的层级；`""` = 全部（不筛）。
   *
   * 它是**服务端分面**：切 tab 会把层级下推成 `apps.list` 的 `scope`，而不是在
   * 浏览器里筛掉已取回的行。行集合真的换了，所以翻页不会翻到刚离开的那个层级里去。
   */
  const [ownerScope, setOwnerScope] = useState<AppOwnerType | "">("")
  /**
   * 真正会被读的那个归属值 —— console 面**结构上**取不到非空值。
   *
   * 这不是重复判断：请求、空态、清空按钮三处都读它，于是「console 不发 `scope`」
   * 是页面的一个不变量，而不是三处各自记得写对的条件。可达集仍由服务端闸门决定，
   * 本页最多只能把它收窄。
   */
  const activeScope: AppOwnerType | "" = isAdmin ? ownerScope : ""
  /**
   * 应用类型筛选（多选，OR）。空集 = 不筛，而不是「什么都不匹配」。
   *
   * 它与归属分面的分工是刻意的：类型是**本地**多选（`apps.list` 没有类型参数，
   * 见 service/app-list-facets.ts），归属是**服务端**单选。二者唯一的共同点在
   * 位置上 —— 都在表格正上方，紧挨着它们要筛的那批行。
   */
  const [kindFilter, setKindFilter] = useState<ReadonlySet<AppKind>>(() => new Set<AppKind>())
  /** 关键字输入框的原文；归一化后的值才进筛选（见 normalizeKeyword）。 */
  const [keywordInput, setKeywordInput] = useState("")
  // 客户端分页页码受控：筛掉一部分行之后，旧页码可能指向一个不存在的分页，
  // 非受控页码会把用户留在空白页上。受控之后筛选变更即回到第 1 页。
  const [tablePage, setTablePage] = useState(1)
  // 创建与发布是两条独立命令：各自开各自的话框，互不代替。
  const [createOpen, setCreateOpen] = useState(false)
  const [publishTarget, setPublishTarget] = useState<AppResponse>()
  // 行内运维命令：编辑 / 源码规格 / 修改源码 / 发布 / 发布历史 / 域名设置 / 详情 / 启停 / 归档，
  // 都挂在行上、都只针对已存在的应用。启停只在该应用处于 ACTIVE 或 PAUSED 时出现 —— 其余状态没有
  // 合法迁移，宁可不给入口也不发一个会被服务端拒绝的请求。
  const [editTarget, setEditTarget] = useState<AppResponse>()
  const [uploadTarget, setUploadTarget] = useState<AppResponse>()
  const [historyTarget, setHistoryTarget] = useState<AppResponse>()
  const [sourceSpecsTarget, setSourceSpecsTarget] = useState<AppResponse>()
  const [domainTarget, setDomainTarget] = useState<AppResponse>()
  const [detailTarget, setDetailTarget] = useState<AppResponse>()
  const [archiveTarget, setArchiveTarget] = useState<AppResponse>()
  const [refresh, setRefresh] = useState(0)

  useEffect(() => {
    let active = true
    setBusy(true)
    setError(undefined)
    // 只有归属分面会下推服务端（`activeScope`）；类型与关键字留在本地
    // （`apps.list` 不接受二者）。可达集仍由服务端的归属闸门决定，`scope` 最多
    // 只能把它收窄，不可能把闸门搬进浏览器。
    //
    // 换分面期间旧行会留到新答案到达（外层 `aria-busy` + 禁用刷新按钮）：这与
    // 「刷新」是同一套 stale-while-revalidate，而不是把表格清空再填 —— 失败时
    // 错误横幅会说明这一批行没换成。
    void loadAllApps((page) => service.listApps({
      page,
      pageSize: APP_LIST_PAGE_SIZE,
      ...(activeScope === "" ? {} : { scope: activeScope }),
    })).then((snapshot) => {
      if (!active) return
      setApps(snapshot.items)
      setTruncated(snapshot.truncated)
    }).catch((cause) => {
      if (active) {
        const message = cause instanceof Error ? cause.message : String(cause)
        setError(t("appsLoadFailed", { message }))
      }
    }).finally(() => {
      if (active) setBusy(false)
    })
    return () => { active = false }
  }, [activeScope, refresh, service, t])

  const refreshList = () => { setRefresh((value) => value + 1) }

  /**
   * 任意一个分面变化 ⇒ 行集合变化 ⇒ 页码回到第 1 页。
   * 共用这一条，免得其中一个漏掉复位。
   */
  const resetPage = () => { setTablePage(1) }

  const toggleKind = (kind: AppKind) => {
    setKindFilter((current) => {
      const next = new Set(current)
      if (next.has(kind)) {
        next.delete(kind)
      } else {
        next.add(kind)
      }
      return next
    })
    resetPage()
  }

  /**
   * 换归属 tab = 换一个行集合。提示横幅先撤掉：上一条回执说的是上一个层级的事，
   * 让它跨层级留在屏幕上会读成「这次切换也成功了」。
   */
  const selectOwnerScope = (scope: AppOwnerType | "") => {
    setNotice(undefined)
    setOwnerScope(scope)
    resetPage()
  }

  /** 清空全部三个分面（归属 + 类型 + 关键字）。 */
  const clearFacets = () => {
    setOwnerScope("")
    setKindFilter(new Set<AppKind>())
    setKeywordInput("")
    resetPage()
  }

  // 类型计数与筛选都建立在这份完整集合上，所以计数与表格永远不会各说各话。
  //
  // 只渲染**有行**的类型：计数为 0 的筛选项点下去只会得到一张空表，是纯噪声。
  // 这在本页是安全的，因为计数取自 `apps`（已加载的完整集合）而类型筛选不回缩
  // `apps` ⇒ 已被选中的类型计数永远 ≥1，芯片不会在选中之后消失、让人再也关不掉
  // 它 —— 那正是 `countAppKinds` 保留零计数项要防的事，而这里的取值基座使它
  // 不可能发生。（关键字只筛 `visibleApps`，同样不影响这份计数。）
  const kindFacets = useMemo(
    () => countAppKinds(apps, APP_KIND_FACET_ORDER).filter((facet) => facet.count > 0),
    [apps],
  )
  const keyword = normalizeKeyword(keywordInput)
  const visibleApps = useMemo(
    () => filterApps(apps, { kinds: kindFilter, keyword }),
    [apps, kindFilter, keyword],
  )
  /**
   * 有没有分面在起作用（归属 / 类型 / 关键字任一）—— 空态文案据此在「还没有应用」和
   * 「筛掉了」之间选，清空按钮也据此出现。
   *
   * 归属必须算进来：admin 选了「平台应用」而平台一个应用都没有时，表格确实是空的，
   * 但那不是「还没有应用」，而「清空筛选」若不撤掉这个 tab 就会是个点不动的按钮。
   */
  const narrowing = activeScope !== "" || kindFilter.size > 0 || keyword !== ""

  /** 三个运维对话框共用同一套「提示 + 关框 + 刷新」收尾。 */
  const settle = (close: () => void, summary?: string) => {
    close()
    if (summary !== undefined) setNotice(summary)
    refreshList()
  }

  /**
   * 列定义：身份（头像 + 名称 + 标识）/ 类型 / 状态 / 归属类型 / 归属用户 /
   * 域名 / 平台目标 / 版本 / 更新时间。
   * 行内八命令走框架的行动作槽，不再手写单元格。
   */
  const columns = useMemo<DataTableColumn<AppResponse>[]>(() => [
    {
      id: "name",
      header: t("columnName"),
      cell: (app) => (
        // 身份单元格：头像 + 名称 + 标识。名称与标识原本是两列 —— 同一个应用
        // 的两行事实被一个竖线隔开，扫读时眼睛要跳两次才能把「它叫什么」和
        // 「它是什么」对上。行业台账（Vercel 项目表、Cloudflare 应用表）的答案
        // 一致：一列身份，图标打头，标识做名称下的次要行。合并后省出的 160px
        // 让十列台账在最常拥挤的域名列上多喘一口气的余地。
        <span className="apps-identity">
          <AppAvatar app={app} resolveIconUrl={resolveIconUrl} />
          <span className="apps-identity-text">
            <strong className="apps-identity-name">{app.name}</strong>
            <code className="apps-identity-slug">{app.slug}</code>
          </span>
        </span>
      ),
      width: 260,
    },
    {
      id: "kind",
      header: t("columnKind"),
      cell: (app) => enumLabel(app.appKind, APP_KIND_LABEL_KEYS, t),
      width: 140,
    },
    {
      id: "status",
      header: t("columnStatus"),
      cell: (app) => (
        <span className={`status-badge status-${app.appStatus.toLowerCase()}`}>
          {enumLabel(app.appStatus, APP_STATUS_LABEL_KEYS, t)}
        </span>
      ),
      width: 110,
    },
    // 归属两列。服务端 `apps.list` 是租户级列表，此前不带任何归属信息，运维无法
    // 区分「平台运维的应用 / 租户共享应用 / 某个人的应用」——同一张表里三者长得
    // 一样。徽标复用根域名台账那套 `scope-badge` 词表（同一个「归属层级」概念），
    // 不另造一套样式。
    {
      id: "ownerType",
      header: t("columnOwnerType"),
      cell: (app) => (
        <span className={`scope-badge scope-badge-${app.ownerType.toLowerCase()}`}>
          {enumLabel(app.ownerType, APP_OWNER_TYPE_LABEL_KEYS, t)}
        </span>
      ),
      width: 120,
    },
    {
      id: "owner",
      header: t("columnOwner"),
      cell: (app) => {
        // `ownerId` 由服务端解析：USER 为 user_id、ORGANIZATION 为组织 id，
        // 平台/租户两级没有单一主体（归属就是层级本身）⇒ 显示层级自己的名字，
        // 而不是把「归属类型」那列的值再抄一遍。
        const ownerId = app.ownerId
        if (ownerId !== undefined && ownerId !== "") {
          // 长 id 折尾：单元格自己的 `max-width` + 省略号由宿主样式兜住，这里
          // 只保证最短可用辨识长度，`title` 留全量值。`owner-id` 是语义锚点 ——
          // 行内还有身份列的 slug `<code>`，按元素选择器取 code 会拿错列。
          const display = ownerId.length > 18 ? `…${ownerId.slice(-12)}` : ownerId
          return <code className="owner-id" title={ownerId}>{display}</code>
        }
        const scopeKey = APP_OWNER_SCOPE_LABEL_KEYS[app.ownerType]
        return <span className="muted">{scopeKey === undefined ? t("detailNoValue") : t(scopeKey)}</span>
      },
      width: 150,
    },
    {
      id: "domains",
      header: t("columnDomains"),
      cell: (app) => {
        const hostname = primaryHostname(app)
        if (hostname === undefined) {
          return <span className="muted">{t("domainNotConfigured")}</span>
        }
        // 后缀目录可能有多条；列表只展示首个，其余折成计数。
        const extraSuffixes = (app.appDomainSuffixes?.length ?? 0) - 1
        return (
          <span className="domain-cell">
            <code>{hostname}</code>
            {extraSuffixes > 0 && <span className="domain-more">+{extraSuffixes}</span>}
          </span>
        )
      },
      width: 260,
    },
    {
      id: "platformTargets",
      header: t("columnPlatformTargets"),
      align: "right",
      cell: (app) => app.platformTargetCount ?? "-",
      width: 130,
    },
    {
      id: "version",
      header: t("columnVersion"),
      cell: (app) => app.latestReleaseTag ?? "-",
      width: 140,
    },
    {
      id: "updated",
      header: t("columnUpdated"),
      cell: (app) => new Date(app.updatedAt).toLocaleString(locale),
      width: 180,
    },
  ], [locale, t, resolveIconUrl])

  /**
   * 启停走 `apps.pause` / `apps.activate` 两条各自的命令（各自要幂等键），不是同一次
   * `update` 的两种载荷。失败落到错误横幅，不静默吞掉；成功只刷新列表 —— 状态徽章本身
   * 就是回执。
   */
  const toggleStatus = (app: AppResponse, next: "ACTIVE" | "PAUSED") => {
    setNotice(undefined)
    setError(undefined)
    const transition = next === "PAUSED" ? service.pauseApp(app.id) : service.activateApp(app.id)
    void transition.then(() => { refreshList() }).catch((cause) => {
      const message = cause instanceof Error ? cause.message : String(cause)
      setError(t("operationFailed", { message }))
    })
  }

  /**
   * 行内八命令，按生命周期排序：编辑 / 源码规格 / 修改源码 / 发布 / 域名设置 / 详情 → 启停 → 归档。
   *
   * 渲染成 `table-action` 图标按钮而不是文字按钮 —— 操作列在 host 的 `deploy-surface.css`
   * 里是 `min-width:144px`，8 个固定 32px 的图标合计 256px 仍在同模块先例内（域名台账每行
   * 5 个动作、其中 2 个还带文字），而同样数量的文字按钮会在窄控制台上把表格顶出横向滚动。
   *
   * 启停只在 ACTIVE / PAUSED 之间出现：其余状态没有合法迁移，宁可不给入口也不发一个注定
   * 被服务端拒绝的请求。归档是不可逆的，红色描边并先开确认框。
   *
   * 图标没有文字，可访问名就是唯一标识 ⇒ `aria-label` / `title` 都带应用名
   * （host 侧的门禁正是按可访问名断言的，不看 textContent）。
   */
  const renderRowActions = (app: AppResponse) => {
    const actions: readonly RowAction[] = [
      { key: "editApp", icon: Pencil, onSelect: () => { setNotice(undefined); setEditTarget(app) } },
      // 源码规格：一个应用可以带多份源码（PC / H5 / 小程序…），并逐端指定默认与回退链。
      // 排在「修改源码」之前 —— 先声明有哪几份源码，再谈往哪一份里上传代码。
      { key: "sourceSpecsAction", icon: Layers, onSelect: () => { setNotice(undefined); setSourceSpecsTarget(app) } },
      { key: "updateSource", icon: Upload, onSelect: () => { setNotice(undefined); setUploadTarget(app) } },
      { key: "publishRelease", icon: Rocket, onSelect: () => { setNotice(undefined); setPublishTarget(app) } },
      // 发布历史紧跟在「发布」之后（见文件头注释）：一次发布做完，下一个问题必然是
      // 「刚才那一版是什么、上一版是什么、退不退得回去」。
      { key: "releaseHistoryAction", icon: History, onSelect: () => { setNotice(undefined); setHistoryTarget(app) } },
      { key: "domainSettingsAction", icon: Globe, onSelect: () => { setNotice(undefined); setDomainTarget(app) } },
      { key: "appDetailAction", icon: Info, onSelect: () => { setNotice(undefined); setDetailTarget(app) } },
      ...(app.appStatus === "ACTIVE"
        ? [{ key: "disableApp", icon: CirclePause, onSelect: () => { toggleStatus(app, "PAUSED") } } satisfies RowAction]
        : app.appStatus === "PAUSED"
          ? [{ key: "enableApp", icon: CirclePlay, onSelect: () => { toggleStatus(app, "ACTIVE") } } satisfies RowAction]
          : []),
      // 归档：契约没有 `DELETE /apps/{appId}`，退役是 `AppStatus.ARCHIVED` 状态迁移。
      // 本控制台无法撤销，所以先开确认框，而不是直接执行。
      { key: "archiveApp", icon: Archive, danger: true, onSelect: () => { setNotice(undefined); setArchiveTarget(app) } },
    ]
    return (
      <div className="row-actions">
        {actions.map((action) => (
          <button
            key={action.key}
            className={action.danger === true ? "table-action danger-action" : "table-action"}
            type="button"
            title={t(action.key)}
            aria-label={`${t(action.key)} ${app.name}`}
            onClick={action.onSelect}
          >
            <action.icon aria-hidden="true" size={16} />
          </button>
        ))}
      </div>
    )
  }

  /**
   * 空态分两种，不能合成一句：表里一条都没有时该请人创建第一个应用，表里有行却
   * 被分面筛空时该请人松开筛选 —— 后者提示「还没有应用」会把一个筛选状态说成
   * 一个数据状态。
   */
  const emptyState = narrowing
    ? (
      <div className="empty-state">
        <p>{t("appsFilteredEmpty")}</p>
        <button type="button" className="command-button" onClick={clearFacets}>
          {t("appsFilteredEmptyAction")}
        </button>
      </div>
    )
    : (
      <div className="empty-state">
        <p>{t("appsEmpty")}</p>
        <button
          type="button"
          className="command-button"
          onClick={() => { setNotice(undefined); setCreateOpen(true) }}
        >
          + {t("createAppAction")}
        </button>
      </div>
    )

  return (
    <section className="resource-page publishing-apps-page">
      <header className="page-header">
        <div>
          <span className="eyebrow">{t("appsPageEyebrow")}</span>
          <h1>{t("appsPageTitle")}</h1>
          <p>{t("appsPageDescription")}</p>
        </div>
        <div className="actions">
          <button type="button" className="command-button" disabled={busy} onClick={refreshList}>
            {t("refresh")}
          </button>
          {/* 主命令是「新增应用」：发布能力只在应用存在之后才出现（行内动作）。 */}
          <button type="button" className="command-button" onClick={() => { setNotice(undefined); setCreateOpen(true) }}>
            + {t("createAppAction")}
          </button>
        </div>
      </header>
      {error && <div className="error-banner" role="alert">{error}</div>}
      {notice && <div className="success-banner" role="status">{notice}</div>}
      {/* 分面全部落在表格正上方、紧挨着它们要筛的那批行 —— 两个面都没有左侧栏。
          admin 比 console 多**一段**归属 tab，那是两个面唯一的分歧，而分歧来自
          「够得到几档归属」，不来自样式：够到四档才值得给控件，够到一档就不给。

          「同排」是刻意的：这三段各筛一个正交的轴（归属 / 类型 / 关键字），拆成两排
          会让人以为后一排是前一排的子级。一排放不下时由工具条整体换行。 */}
      <div className="apps-ledger">
        <div className="apps-ledger-toolbar">
          {isAdmin && (
            <div className="apps-owner-facets">
              {/* 归属 tab 条：单选，且是**服务端**分面 —— 切 tab 把层级下推成 `scope`，
                  行集合真的换了。它刻意不带计数：那需要第二趟请求，而且与本控件
                  「换一个视图」的语义不符（类型芯片带计数是因为它的计数与筛选同源，
                  见 service/app-list-facets.ts）。

                  `role="tab"` 换的是同一个表格的行集合，没有各自的 `aria-controls`
                  面板：这里真正表达的是 `aria-selected`（哪一档现在生效）。

                  不带可见标签：首枚 tab 就是「全部归属类型」，轴名已由 tab 文案自明，
                  再挂一个「归属类型」只是复读。group 名由 `aria-label` 承担 ——
                  tab 条仍是被命名的控件，只是名字不占版面。 */}
              <div className="apps-owner-tabs" role="tablist" aria-label={t("ownerTypeFilter")}>
                <button
                  type="button"
                  role="tab"
                  className="apps-owner-tab"
                  aria-selected={activeScope === ""}
                  data-owner-scope=""
                  onClick={() => { selectOwnerScope("") }}
                >
                  {t("ownerTypeFilterAll")}
                </button>
                {APP_OWNER_SCOPE_OPTIONS.map((scope) => (
                  <button
                    key={scope}
                    type="button"
                    role="tab"
                    className="apps-owner-tab"
                    aria-selected={activeScope === scope}
                    data-owner-scope={scope}
                    onClick={() => { selectOwnerScope(scope) }}
                  >
                    {t(APP_OWNER_TYPE_LABEL_KEYS[scope])}
                  </button>
                ))}
              </div>
            </div>
          )}
          {/* 关键字与类型同侧（都是客户端过滤）：二者都建立在这份已取全的集合上，
              所以「筛选」与「计数」不会分别基于不同的行集。 */}
          <label className="search-box">
            <Search aria-hidden="true" size={15} />
            <input
              type="search"
              aria-label={t("appSearchLabel")}
              placeholder={t("appSearchPlaceholder")}
              value={keywordInput}
              onChange={(event) => { setKeywordInput(event.target.value); resetPage() }}
            />
            {keywordInput !== "" && (
              <button
                type="button"
                className="apps-search-clear"
                title={t("appSearchClear")}
                aria-label={t("appSearchClear")}
                onClick={() => { setKeywordInput(""); resetPage() }}
              >
                <X aria-hidden="true" size={14} />
              </button>
            )}
          </label>
          {/* 类型是**多选 OR**：芯片自己带计数与开关状态（`aria-pressed`），所以
              「现在筛的是什么」不用展开任何东西就能看见——这正是当初那个折叠
              `<select>` 做不到、因而被换掉的属性。
              只列有行的类型；计数取自完整集合、类型筛选不回缩它 ⇒ 已选中的芯片
              不会在选中后消失。 */}
          {kindFacets.length > 0 && (
            <div className="apps-kind-facets" role="group" aria-labelledby="publishing-apps-kind-label">
              <span className="apps-facet-label" id="publishing-apps-kind-label">{t("appTypeFilter")}</span>
              {kindFacets.map((facet) => (
                <button
                  key={facet.appKind}
                  type="button"
                  className="apps-kind-chip"
                  aria-pressed={kindFilter.has(facet.appKind)}
                  data-app-kind={facet.appKind}
                  title={t(APP_KIND_LABEL_KEYS[facet.appKind])}
                  onClick={() => { toggleKind(facet.appKind) }}
                >
                  {t(APP_KIND_LABEL_KEYS[facet.appKind])}
                  <span className="apps-kind-chip-count">{facet.count}</span>
                </button>
              ))}
            </div>
          )}
          {/* 「清空」只在真有分面在用时出现：一个恒可点的按钮会让人怀疑面板里是不是
              还有看不见的条件。它撤的是**全部三个**分面（归属 tab 一起回「全部」），
              否则点了它表格可能还是空的，而按钮已经没了。 */}
          {narrowing && (
            <button type="button" className="apps-facet-clear" onClick={clearFacets}>
              {t("appTypeFilterClear")}
            </button>
          )}
          <span className="apps-ledger-summary" aria-live="polite">
            {t("appsShownCount", { shown: visibleApps.length, total: apps.length })}
          </span>
        </div>
        {truncated && <p className="apps-ledger-note">{t("appsTruncatedNote", { count: apps.length })}</p>}
        {/* 「为什么筛不到 H5」只在类型筛选真的在用的时候解释；常驻就成了噪声。 */}
        {kindFilter.size > 0 && <p className="apps-ledger-note">{t("appTypeFilterHint")}</p>}
        <div aria-busy={busy}>
          <DataTable<AppResponse>
            columns={columns}
            density="compact"
            emptyState={emptyState}
            getRowId={(app) => app.id}
            loading={busy && apps.length === 0}
            pagination={{
              defaultPageSize: 20,
              mode: "client",
              // 受控页码：见 `resetPage` —— 行集合变化时必须能回到第 1 页。
              page: tablePage,
              pageSizeOptions: APP_TABLE_PAGE_SIZE_OPTIONS,
              onPageChange: setTablePage,
            }}
            rowActions={renderRowActions}
            rowActionsLabel={t("operations")}
            rows={visibleApps}
            stickyHeader
          />
        </div>
      </div>
      {createOpen && (
        <CreateAppDialog
          deployClient={deployClient}
          driveClient={driveClient}
          locale={locale}
          variant="drawer"
          size="lg"
          onClose={() => { setCreateOpen(false) }}
          onCreated={(app) => {
            setCreateOpen(false)
            setNotice(t("createAppSucceeded", { name: app.name }))
            refreshList()
          }}
        />
      )}
      {publishTarget !== undefined && (
        // `onSaved` refreshes but deliberately does not close: the dialog reports
        // the accepted command and the deployment's own status (`PRD-FR-026`), and
        // that is worth reading rather than being flashed past on auto-close.
        <AppPublishDialog
          app={publishTarget}
          locale={locale}
          service={service}
          onClose={() => { setPublishTarget(undefined) }}
          onSaved={() => { refreshList() }}
        />
      )}
      {editTarget !== undefined && (
        <AppEditDialog
          app={editTarget}
          locale={locale}
          service={service}
          onClose={() => { setEditTarget(undefined) }}
          onSaved={() => { settle(() => { setEditTarget(undefined) }) }}
        />
      )}
      {archiveTarget !== undefined && (
        <AppArchiveDialog
          app={archiveTarget}
          locale={locale}
          service={service}
          onClose={() => { setArchiveTarget(undefined) }}
          onArchived={() => { settle(() => { setArchiveTarget(undefined) }) }}
        />
      )}
      {uploadTarget !== undefined && (
        <UploadSourceDialog
          deployClient={deployClient}
          driveClient={driveClient}
          locale={locale}
          app={uploadTarget}
          onClose={() => { setUploadTarget(undefined) }}
          // 这个应用一份规格都还没有时，对话框的「绑定源码来源」无处可绑。与其
          // 让操作员在空下拉前卡住，不如把落点直接交出去：换的是**同一个应用**的
          // 另一个命令（源码规格），而不是另开一个应用。
          onRequestSpecs={() => { setUploadTarget(undefined); setSourceSpecsTarget(uploadTarget) }}
          onUploaded={(summary) => { settle(() => { setUploadTarget(undefined) }, summary) }}
        />
      )}
      {sourceSpecsTarget !== undefined && (
        <AppSourceSpecsDrawer
          deployClient={deployClient}
          driveClient={driveClient}
          locale={locale}
          app={sourceSpecsTarget}
          onClose={() => { setSourceSpecsTarget(undefined) }}
          // 抽屉内的每次落库都会改规格清单或每端默认 ⇒ 关框时刷新列表
          // （列表本身不显示规格，但详情抽屉会，保持一致）。
          onChanged={(summary) => { setNotice(summary); refreshList() }}
        />
      )}
      {historyTarget !== undefined && (
        // 历史里可以发起回滚（一次写操作）。回滚成功只改「部署」这一段，列表上的
        // 版本列要跟着走，所以关框时刷新 —— 与源码规格抽屉同一条理由。
        <AppReleaseHistoryDrawer
          deployClient={deployClient}
          driveClient={driveClient}
          locale={locale}
          app={historyTarget}
          onClose={() => { setHistoryTarget(undefined); refreshList() }}
        />
      )}
      {domainTarget !== undefined && (
        <AppDomainDialog
          deployClient={deployClient}
          driveClient={driveClient}
          locale={locale}
          app={domainTarget}
          onClose={() => { setDomainTarget(undefined) }}
          onChanged={(summary) => { settle(() => { setDomainTarget(undefined) }, summary) }}
        />
      )}
      {detailTarget !== undefined && (
        <AppDetailDrawer
          deployClient={deployClient}
          driveClient={driveClient}
          locale={locale}
          app={detailTarget}
          onClose={() => { setDetailTarget(undefined) }}
        />
      )}
    </section>
  )
}
