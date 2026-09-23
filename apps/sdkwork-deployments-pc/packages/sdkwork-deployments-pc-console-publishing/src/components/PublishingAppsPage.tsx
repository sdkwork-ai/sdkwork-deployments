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
 *   4. **Delete** — rendered as a permanently disabled slot: the deploy app-api
 *      contract defines no `apps.delete`, so the `title` says why instead of
 *      promising a call the contract cannot make.
 *
 * Publishing is therefore unavailable until an app exists: publishing is a
 * per-app row action, and with an empty table there is nothing to publish.
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
import type { AppKind, AppResponse, AppStatus, SdkworkDeployAppClient } from "@sdkwork/deployments-app-sdk";
import type { SdkworkDriveAppClient } from "@sdkwork/drive-app-sdk";
import type { DeploymentsLocale } from "@sdkwork/deployments-pc-commons";
import { Globe, Info, Pencil, Rocket, Trash2, Upload, type LucideIcon } from "lucide-react";
import {
  publishingTranslator,
  APP_KIND_LABEL_KEYS,
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
import { AppEditDialog } from "./AppOperationsDialogs.tsx";
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

/** 行内运维命令的一条：`onSelect` 缺省即「保留槽位但不可用」。 */
interface RowAction {
  readonly key: PublishingMessageKey
  readonly icon: LucideIcon
  readonly onSelect?: () => void
  /** 覆写 hover 提示（默认与 `key` 同）；禁用槽位用它说明「为什么不能」而非重复「是什么」。 */
  readonly titleKey?: PublishingMessageKey
}

/** 枚举 → 本地化文案；映射表未覆盖的新枚举值回退原文。 */
function enumLabel(kind: AppKind | AppStatus, table: Readonly<Record<string, PublishingMessageKey>>, t: PublishingTranslator): string {
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
  // 创建与发布是两条独立命令：各自开各自的话框，互不代替。
  const [createOpen, setCreateOpen] = useState(false)
  const [publishTarget, setPublishTarget] = useState<AppResponse>()
  // 行内运维命令：编辑 / 修改源码 / 发布 / 域名设置 / 详情，都挂在行上、都只针对已存在的应用。
  // 「删除」只渲染禁用占位 —— 契约的 apps 资源没有 delete（见 removeAppUnavailable）。
  const [editTarget, setEditTarget] = useState<AppResponse>()
  const [uploadTarget, setUploadTarget] = useState<AppResponse>()
  const [domainTarget, setDomainTarget] = useState<AppResponse>()
  const [detailTarget, setDetailTarget] = useState<AppResponse>()
  const [refresh, setRefresh] = useState(0)

  useEffect(() => {
    let active = true
    setBusy(true)
    setError(undefined)
    void service.listApps({ page: 1, pageSize: APP_LIST_PAGE_SIZE }).then((result) => {
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
  }, [refresh, service, t])

  const refreshList = () => { setRefresh((value) => value + 1) }

  /** 三个运维对话框共用同一套「提示 + 关框 + 刷新」收尾。 */
  const settle = (close: () => void, summary?: string) => {
    close()
    if (summary !== undefined) setNotice(summary)
    refreshList()
  }

  /**
   * 列定义：与既有表头一一对应（名称/标识/类型/状态/域名/平台目标/版本/更新时间）。
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
   * 行内六命令，按生命周期排序：编辑 / 修改源码 / 发布 / 域名设置 / 详情 / 删除。
   *
   * 渲染成 `table-action` 图标按钮而不是文字按钮 —— 操作列在 host 的 `deploy-surface.css`
   * 里是 `min-width:144px`，6 个固定 32px 的图标合计 207px 仍在同模块先例内（域名台账每行
   * 5 个动作），而 6 个文字按钮的 min-content 会在窄控制台上把表格顶出横向滚动。
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
      // 删除：契约的 apps 资源没有 delete，所以槽位保留、动作永久禁用，
      // 由 title 说明原因（`removeAppUnavailable`）而不是假装能删。
      { key: "removeApp", icon: Trash2, titleKey: "removeAppUnavailable" },
    ]
    return (
      <div className="row-actions">
        {actions.map((action) => {
          const unavailable = action.onSelect === undefined
          return (
            <button
              key={action.key}
              className={unavailable ? "table-action danger-action" : "table-action"}
              type="button"
              title={t(action.titleKey ?? action.key)}
              aria-label={`${t(action.key)} ${app.name}`}
              disabled={unavailable}
              onClick={action.onSelect}
            >
              <action.icon aria-hidden="true" size={16} />
            </button>
          )
        })}
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
            pageSizeOptions: APP_TABLE_PAGE_SIZE_OPTIONS,
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
