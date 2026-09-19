/**
 * Console-facing apps page: lists tenant deploy_app records, and exposes the
 * application lifecycle as three separate row commands:
 *
 *   1. **Create** — `CreateAppDialog` registers a `deploy_app` with its own
 *      identity only (name / type / category / icon / cover / preview images).
 *      It never touches a source directory or a release.
 *   2. **Publish** — once the app exists, `CreateDeployAppDialog` publishes a
 *      chosen source directory *onto* that app.
 *   3. **Operate** — `UploadSourceDialog` ships code from a local archive, a Git
 *      repository, or an existing Drive archive; `AppDomainDialog` configures the
 *      platform hostname (`appId.app.<suffix>`) and custom domains; and
 *      `AppDetailDrawer` shows the app's gathered facts read-only.
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
import type { AppKind, AppResponse, AppStatus, SdkworkDeployAppClient } from "@sdkwork/deployments-app-sdk";
import type { SdkworkDriveAppClient } from "@sdkwork/drive-app-sdk";
import type { DeploymentsLocale } from "@sdkwork/deployments-pc-commons";
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
import "./create-deploy-app.module.css";

export interface PublishingAppsPageProps {
  readonly deployClient: SdkworkDeployAppClient
  readonly driveClient: SdkworkDriveAppClient
  readonly locale: DeploymentsLocale
  /** Host directory-picker port (optional; falls back to manual path input). */
  readonly pickDirectory?: (current: string | undefined) => Promise<string | undefined>
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
  // 运维三命令：上传代码 / 域名设置 / 详情，都挂在行上、都只针对已存在的应用。
  const [uploadTarget, setUploadTarget] = useState<AppResponse>()
  const [domainTarget, setDomainTarget] = useState<AppResponse>()
  const [detailTarget, setDetailTarget] = useState<AppResponse>()
  const [refresh, setRefresh] = useState(0)

  useEffect(() => {
    let active = true
    setBusy(true)
    setError(undefined)
    void service.listApps({ page: 1, pageSize: 50 }).then((result) => {
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
      <div className="table-frame" aria-busy={busy}>
        <table>
          <thead>
            <tr>
              <th>{t("columnName")}</th>
              <th>{t("columnSlug")}</th>
              <th>{t("columnKind")}</th>
              <th>{t("columnStatus")}</th>
              <th>{t("columnDomains")}</th>
              <th>{t("columnPlatformTargets")}</th>
              <th>{t("columnVersion")}</th>
              <th>{t("columnUpdated")}</th>
              <th>{t("columnActions")}</th>
            </tr>
          </thead>
          <tbody>
            {apps.map((app) => {
              const hostname = primaryHostname(app)
              const extraSuffixes = (app.appDomainSuffixes?.length ?? 0) - 1
              return (
                <tr key={app.id}>
                  <td><strong>{app.name}</strong></td>
                  <td>{app.slug}</td>
                  <td>{enumLabel(app.appKind, APP_KIND_LABEL_KEYS, t)}</td>
                  <td><span className={`status-badge status-${app.appStatus.toLowerCase()}`}>{enumLabel(app.appStatus, APP_STATUS_LABEL_KEYS, t)}</span></td>
                  <td>
                    {hostname === undefined
                      ? <span className="muted">{t("domainNotConfigured")}</span>
                      : (
                        <span className="domain-cell">
                          <code>{hostname}</code>
                          {/* 后缀目录可能有多条；列表只展示首个，其余折成计数。 */}
                          {extraSuffixes > 0 && (
                            <span className="domain-more">+{extraSuffixes}</span>
                          )}
                        </span>
                      )}
                  </td>
                  <td>{app.platformTargetCount ?? "-"}</td>
                  <td>{app.latestReleaseTag ?? "-"}</td>
                  <td>{new Date(app.updatedAt).toLocaleString(locale)}</td>
                  <td>
                    <div className="row-actions">
                      {/* 发布是行内动作 —— 只有已经存在的应用才可能被发布。 */}
                      <button
                        type="button"
                        className="command-button"
                        onClick={() => { setNotice(undefined); setPublishTarget(app) }}
                      >
                        {t("publishAppAction")}
                      </button>
                      <button
                        type="button"
                        className="command-button"
                        onClick={() => { setNotice(undefined); setUploadTarget(app) }}
                      >
                        {t("uploadCodeAction")}
                      </button>
                      <button
                        type="button"
                        className="command-button"
                        onClick={() => { setNotice(undefined); setDomainTarget(app) }}
                      >
                        {t("domainSettingsAction")}
                      </button>
                      <button
                        type="button"
                        className="command-button"
                        onClick={() => { setNotice(undefined); setDetailTarget(app) }}
                      >
                        {t("appDetailAction")}
                      </button>
                    </div>
                  </td>
                </tr>
              )
            })}
          </tbody>
        </table>
        {/* 空态即唯一的创建入口提示：表格内联，不再到表格上方重复一遍。 */}
        {!busy && apps.length === 0 && (
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
