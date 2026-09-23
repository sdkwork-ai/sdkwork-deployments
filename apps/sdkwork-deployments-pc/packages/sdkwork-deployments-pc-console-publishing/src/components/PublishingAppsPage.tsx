/**
 * Console-facing apps page: lists tenant deploy_app records, and exposes the
 * application lifecycle as one header command (create) plus six row commands:
 *
 *   1. **Create** — `CreateAppDialog` registers a `deploy_app` with its own
 *      identity only (name / type / category / icon / cover / preview images).
 *      It never touches a source directory or a release.
 *   2. **Publish** — once the app exists, `CreateDeployAppDialog` publishes a
 *      chosen source directory *onto* that app.
 *   3. **Operate** — `AppEditDialog` edits the app's own metadata;
 *      `UploadSourceDialog` ships code from a local archive, a Git repository, or
 *      an existing Drive archive; `AppDomainDialog` configures the platform
 *      hostname (`appId.app.<suffix>`) and custom domains; and `AppDetailDrawer`
 *      shows the app's gathered facts read-only.
 *   4. **Lifecycle** — `pauseApp` / `activateApp` drive the ACTIVE ↔ PAUSED
 *      transition, and `AppArchiveDialog` retires the app via `apps.update` to
 *      `AppStatus.ARCHIVED`. The app-api defines no `DELETE` route, so
 *      retirement *is* that archival transition rather than a hard delete.
 *
 * Publishing is therefore unavailable until an app exists: publishing is a
 * per-app row action, and with an empty table there is nothing to publish.
 *
 * **Ownership is two columns, not one.** `apps.list` is tenant-wide, so without
 * them a platform-operated app, a tenant's shared app and one person's personal
 * app are indistinguishable in the ledger. `归属类型` names the level and
 * `归属用户` names the concrete owner — a user id for `USER`, an organization id
 * for `ORGANIZATION`, and the level's own name for the two tenant-wide levels
 * that have no single owner, so the level is never printed twice. The header
 * filter drives the server's `scope` facet; the server keeps ownership gating
 * (who may *reach* the app) separate from that facet (whether the app is *of
 * the requested kind*), so filtering never widens visibility.
 *
 * **The domain column needs no extra request.** `AppResponse` already carries
 * the *effective* `appDomainLabel` and `appDomainSuffixes`, so the canonical
 * production hostname is derived locally with the same rule the server uses
 * (`<label>.app.<suffix>`, see `sdkwork-deploy-core::default_app_hostname`).
 * Fetching each row's hostnames instead would be an N+1 request on every list
 * load for a string that is already deterministic.
 *
 * Clients arrive as props (no console-core context dependency), so the same
 * page can be embedded by any host that can construct the two generated
 * clients — the deployments console shell and the BirdCoder plugin alike.
 *
 * 全部用户可见文案走 publishingTranslator 目录（I18N_SPEC v2.0 §1：用户可见
 * 文本必须来自 message catalog）；枚举值经 APP_KIND/APP_STATUS 映射表本地化，
 * 未覆盖的新枚举值回退原文展示。
 */
import { useEffect, useMemo, useState } from "react";
import { DataTable, type DataTableColumn } from "@sdkwork/ui-pc-react";
import type { AppKind, AppOwnerType, AppResponse, AppStatus, SdkworkDeployAppClient } from "@sdkwork/deployments-pc-console-core/sdk";
import type { SdkworkDriveAppClient } from "@sdkwork/deployments-pc-console-core/sdk";
import type { DeploymentsLocale } from "@sdkwork/deployments-pc-commons";
import { Archive, CirclePause, CirclePlay, Globe, Info, Pencil, Rocket, Upload, type LucideIcon } from "lucide-react";
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
import { AppDetailDrawer } from "./AppDetailDrawer.tsx";
import { AppDomainDialog } from "./AppDomainDialog.tsx";
import { CreateAppDialog } from "./CreateAppDialog.tsx";
import { CreateDeployAppDialog } from "./CreateDeployAppDialog.tsx";
import { UploadSourceDialog } from "./UploadSourceDialog.tsx";
import { AppArchiveDialog, AppEditDialog } from "./AppOperationsDialogs.tsx";
import "./create-deploy-app.module.css";

export interface PublishingAppsPageProps {
  readonly deployClient: SdkworkDeployAppClient
  readonly driveClient: SdkworkDriveAppClient
  readonly locale: DeploymentsLocale
  /** Host directory-picker port (optional; falls back to manual path input). */
  readonly pickDirectory?: (current: string | undefined) => Promise<string | undefined>
}

/** 列表拉取上限：一次取满一页候选全集，之后由 DataTable 在客户端分页。 */
const APP_LIST_PAGE_SIZE = 50;
const APP_TABLE_PAGE_SIZE_OPTIONS = [20, 50, 100] as const;

/**
 * 归属类型筛选项，按可见性由宽到窄排列（`PLATFORM` → `USER`，与后端
 * `AppOwnerType::rank` / `scope_rank` 同序）。
 *
 * 键取自 {@link APP_OWNER_TYPE_LABEL_KEYS} 而不是手写一份：那张表是
 * `Record<AppOwnerType, …>`，服务端新增归属层级时它必编译错，筛选项因此自动
 * 跟上，不会出现「枚举加了、筛选项漏了」的半套。
 */
const APP_OWNER_SCOPE_OPTIONS = Object.keys(APP_OWNER_TYPE_LABEL_KEYS) as readonly AppOwnerType[];

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

export function PublishingAppsPage({ deployClient, driveClient, locale, pickDirectory }: PublishingAppsPageProps) {
  const t = useMemo(() => publishingTranslator(locale), [locale])
  const service = useMemo(
    () => createDeployAppPublishingService({ deployClient, driveClient }),
    [deployClient, driveClient],
  )
  const [apps, setApps] = useState<AppResponse[]>([])
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string>()
  const [notice, setNotice] = useState<string>()
  /** 归属类型筛选。空串 = 不筛 —— 服务端仍按归属闸门返回可达集合，二者独立。 */
  const [ownerScope, setOwnerScope] = useState<AppOwnerType | "">("")
  // 客户端分页页码受控：筛掉一部分行之后，旧页码可能指向一个不存在的分页，
  // 非受控页码会把用户留在空白页上。受控之后筛选变更即回到第 1 页。
  const [tablePage, setTablePage] = useState(1)
  // 创建与发布是两条独立命令：各自开各自的话框，互不代替。
  const [createOpen, setCreateOpen] = useState(false)
  const [publishTarget, setPublishTarget] = useState<AppResponse>()
  // 行内运维命令：编辑 / 修改源码 / 发布 / 域名设置 / 详情 / 启停 / 归档，都挂在行上、
  // 都只针对已存在的应用。启停只在该应用处于 ACTIVE 或 PAUSED 时出现 —— 其余状态没有
  // 合法迁移，宁可不给入口也不发一个会被服务端拒绝的请求。
  const [editTarget, setEditTarget] = useState<AppResponse>()
  const [uploadTarget, setUploadTarget] = useState<AppResponse>()
  const [domainTarget, setDomainTarget] = useState<AppResponse>()
  const [detailTarget, setDetailTarget] = useState<AppResponse>()
  const [archiveTarget, setArchiveTarget] = useState<AppResponse>()
  const [refresh, setRefresh] = useState(0)

  useEffect(() => {
    let active = true
    setBusy(true)
    setError(undefined)
    console.log("PAGE-DIAG ownerScope=", JSON.stringify(ownerScope), "call=", JSON.stringify({
      page: 1,
      pageSize: APP_LIST_PAGE_SIZE,
      ...(ownerScope === "" ? {} : { scope: ownerScope }),
    }))
    void service.listApps({
      page: 1,
      pageSize: APP_LIST_PAGE_SIZE,
      ...(ownerScope === "" ? {} : { scope: ownerScope }),
    }).then((result) => {
      if (active) setApps(result.items)
    }).catch((cause) => {
      if (active) {
        const message = cause instanceof Error ? cause.message : String(cause)
        setError(t("appsLoadFailed", { message }))
      }
    }).finally(() => {
      if (active) setBusy(false)
    })
    return () => { active = false }
  }, [ownerScope, refresh, service, t])

  const refreshList = () => { setRefresh((value) => value + 1) }

  /** 换筛选分面 = 换一个行集合 ⇒ 页码必须回到第 1 页，否则可能停在空页上。 */
  const selectOwnerScope = (value: AppOwnerType | "") => {
    setOwnerScope(value)
    setTablePage(1)
  }

  /** 三个运维对话框共用同一套「提示 + 关框 + 刷新」收尾。 */
  const settle = (close: () => void, summary?: string) => {
    close()
    if (summary !== undefined) setNotice(summary)
    refreshList()
  }

  /**
   * 列定义：与既有表头一一对应（名称 / 标识 / 类型 / 状态 / 归属类型 / 归属用户 /
   * 域名 / 平台目标 / 版本 / 更新时间）。
   * 行内六命令（编辑/修改源码/发布/域名设置/详情/删除占位）走框架的行动作槽，不再手写单元格。
   */
  const columns = useMemo<DataTableColumn<AppResponse>[]>(() => [
    {
      id: "name",
      header: t("columnName"),
      cell: (app) => <strong>{app.name}</strong>,
      width: 180,
    },
    {
      id: "slug",
      header: t("columnSlug"),
      cell: (app) => app.slug,
      width: 160,
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
          // 只保证最短可用辨识长度，`title` 留全量值。
          const display = ownerId.length > 18 ? `…${ownerId.slice(-12)}` : ownerId
          return <code title={ownerId}>{display}</code>
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
  ], [locale, t])

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
   * 行内六到七命令，按生命周期排序：编辑 / 修改源码 / 发布 / 域名设置 / 详情 → 启停 → 归档。
   *
   * 渲染成 `table-action` 图标按钮而不是文字按钮 —— 操作列在 host 的 `deploy-surface.css`
   * 里是 `min-width:144px`，7 个固定 32px 的图标合计 242px 仍在同模块先例内（域名台账每行
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
      { key: "updateSource", icon: Upload, onSelect: () => { setNotice(undefined); setUploadTarget(app) } },
      { key: "publishRelease", icon: Rocket, onSelect: () => { setNotice(undefined); setPublishTarget(app) } },
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

  return (
    <section className="resource-page publishing-apps-page">
      <header className="page-header">
        <div>
          <span className="eyebrow">{t("appsPageEyebrow")}</span>
          <h1>{t("appsPageTitle")}</h1>
          <p>{t("appsPageDescription")}</p>
        </div>
        <div className="actions">
          {/* 归属类型筛选走服务端 `scope` 分面（不是本地过滤）：列表可能不止一页，
              本地过滤会让「筛出来的结果」随分页变化而漂移。 */}
          <label className="scope-filter">
            <span className="scope-filter-label">{t("ownerTypeFilter")}</span>
            <select
              aria-label={t("ownerTypeFilter")}
              value={ownerScope}
              onChange={(event) => { selectOwnerScope(event.target.value as AppOwnerType | "") }}
            >
              <option value="">{t("ownerTypeFilterAll")}</option>
              {APP_OWNER_SCOPE_OPTIONS.map((scope) => (
                <option key={scope} value={scope}>{t(APP_OWNER_TYPE_LABEL_KEYS[scope])}</option>
              ))}
            </select>
          </label>
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
      {/* 空态只在表格内呈现（appsEmpty）—— 表格上方不再重复「请先创建应用」提示。 */}
      <div aria-busy={busy}>
        <DataTable<AppResponse>
          columns={columns}
          density="compact"
          emptyState={(
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
          )}
          getRowId={(app) => app.id}
          loading={busy && apps.length === 0}
          pagination={{
            defaultPageSize: 20,
            mode: "client",
            // 受控页码：见 `selectOwnerScope` —— 行集合变化时必须能回到第 1 页。
            page: tablePage,
            pageSizeOptions: APP_TABLE_PAGE_SIZE_OPTIONS,
            onPageChange: setTablePage,
          }}
          rowActions={renderRowActions}
          rowActionsLabel={t("operations")}
          rows={apps}
          stickyHeader
        />
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
        <CreateDeployAppDialog
          deployClient={deployClient}
          driveClient={driveClient}
          locale={locale}
          pickDirectory={pickDirectory}
          publishApp={publishTarget}
          onClose={() => { setPublishTarget(undefined) }}
          onPublished={() => {
            setPublishTarget(undefined)
            refreshList()
          }}
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
          onUploaded={(summary) => { settle(() => { setUploadTarget(undefined) }, summary) }}
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
