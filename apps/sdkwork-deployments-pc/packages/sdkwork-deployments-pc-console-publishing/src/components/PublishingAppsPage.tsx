/**
 * Console-facing apps page: lists tenant deploy_app records and opens the
 * CreateDeployAppDialog from a "Publish" command.
 *
 * Clients arrive as props (no console-core context dependency), so the same
 * page can be embedded by any host that can construct the two generated
 * clients — the deployments console shell and the BirdCoder plugin alike.
 *
 * 全部用户可见文案走 publishingTranslator 目录（I18N_SPEC v2.0 §1：用户可见
 * 文本必须来自 message catalog）；枚举值经 APP_KIND/APP_STATUS 映射表本地化，
 * 未覆盖的新枚举值回退原文展示。
 */
import { Pencil, Rocket, Trash2, Upload } from "lucide-react";
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
import { AppEditDialog, AppPublishDialog, AppSourceDialog } from "./AppOperationsDialogs.tsx";
import { CreateDeployAppDialog } from "./CreateDeployAppDialog.tsx";
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

export function PublishingAppsPage({ deployClient, driveClient, locale, pickDirectory }: PublishingAppsPageProps) {
  const t = useMemo(() => publishingTranslator(locale), [locale])
  const service = useMemo(
    () => createDeployAppPublishingService({ deployClient, driveClient }),
    [deployClient, driveClient],
  )
  const [apps, setApps] = useState<AppResponse[]>([])
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string>()
  const [createOpen, setCreateOpen] = useState(false)
  const [refresh, setRefresh] = useState(0)
  /**
   * The row operation currently open, if any.
   *
   * Held here rather than in the row so the ledger has exactly one dialog
   * mounted at a time, and so the record it acts on survives the reload the
   * operation triggers.
   */
  const [operation, setOperation] = useState<{ kind: "edit" | "source" | "publish"; app: AppResponse }>()
  const reload = () => { setRefresh((value) => value + 1) }
  const closeOperation = () => { setOperation(undefined) }
  const finishOperation = () => {
    setOperation(undefined)
    reload()
  }

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

  return (
    <section className="resource-page publishing-apps-page">
      <header className="page-header">
        <div>
          <span className="eyebrow">{t("appsPageEyebrow")}</span>
          <h1>{t("appsPageTitle")}</h1>
          <p>{t("appsPageDescription")}</p>
        </div>
        <div className="actions">
          <button type="button" className="command-button" disabled={busy} onClick={() => { setRefresh((value) => value + 1) }}>
            {t("refresh")}
          </button>
          <button type="button" className="command-button" onClick={() => { setCreateOpen(true) }}>
            + {t("publishApp")}
          </button>
        </div>
      </header>
      {error && <div className="error-banner" role="alert">{error}</div>}
      <div className="table-frame" aria-busy={busy}>
        <table>
          <thead>
            <tr>
              <th>{t("columnName")}</th>
              <th>{t("columnSlug")}</th>
              <th>{t("columnKind")}</th>
              <th>{t("columnStatus")}</th>
              <th>{t("columnPlatformTargets")}</th>
              <th>{t("columnVersion")}</th>
              <th>{t("columnUpdated")}</th>
              <th className="operations-column">{t("operations")}</th>
            </tr>
          </thead>
          <tbody>
            {apps.map((app) => (
              <tr key={app.id}>
                <td><strong>{app.name}</strong></td>
                <td>{app.slug}</td>
                <td>{enumLabel(app.appKind, APP_KIND_LABEL_KEYS, t)}</td>
                <td><span className={`status-badge status-${app.appStatus.toLowerCase()}`}>{enumLabel(app.appStatus, APP_STATUS_LABEL_KEYS, t)}</span></td>
                <td>{app.platformTargetCount ?? "-"}</td>
                <td>{app.latestReleaseTag ?? "-"}</td>
                <td>{new Date(app.updatedAt).toLocaleString(locale)}</td>
                {/*
                  Row operations, rendered in the module's own operations-column
                  idiom: `row-actions` + `table-action` icon buttons, exactly as
                  `DeliveryManagement.tsx` renders the domains, hostnames and
                  certificates ledgers.

                  The earlier text-label variant was measured out. Four nowrap
                  text buttons ("Edit" / "Modify source code" / "Publish" /
                  "Delete") gave the column a 327.8px min-content width, against
                  the 144px this stylesheet authors for `.operations-column` —
                  four 32px icon buttons plus gaps. At a real console content
                  width of ~1036px that pushed the frame into horizontal scroll
                  and squeezed Status/Version under 80px. Icons restore the
                  authored width.

                  `delete` is present but permanently disabled. The deploy app-api
                  contract defines no `apps.delete` (only zones, hostnames,
                  certificates and artifacts are deletable), so rendering an
                  enabled button would promise a call that cannot be made; the
                  title carries the reason instead of the capability being
                  silently missing.
                */}
                <td>
                  <div className="row-actions">
                    <button className="table-action" type="button" title={t("editApp")} aria-label={`${t("editApp")} ${app.name}`} onClick={() => setOperation({ app, kind: "edit" })}><Pencil aria-hidden="true" size={16} /></button>
                    <button className="table-action" type="button" title={t("updateSource")} aria-label={`${t("updateSource")} ${app.name}`} onClick={() => setOperation({ app, kind: "source" })}><Upload aria-hidden="true" size={16} /></button>
                    <button className="table-action" type="button" title={t("publishRelease")} aria-label={`${t("publishRelease")} ${app.name}`} onClick={() => setOperation({ app, kind: "publish" })}><Rocket aria-hidden="true" size={16} /></button>
                    <button className="table-action danger-action" disabled type="button" title={t("removeAppUnavailable")} aria-label={`${t("removeApp")} ${app.name}`}><Trash2 aria-hidden="true" size={16} /></button>
                  </div>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
        {!busy && apps.length === 0 && <div className="empty-state">{t("appsEmpty")}</div>}
      </div>
      {createOpen && (
        <CreateDeployAppDialog
          deployClient={deployClient}
          driveClient={driveClient}
          locale={locale}
          pickDirectory={pickDirectory}
          onClose={() => { setCreateOpen(false) }}
          onPublished={() => {
            setCreateOpen(false)
            reload()
          }}
        />
      )}
      {operation?.kind === "edit" && (
        <AppEditDialog app={operation.app} locale={locale} service={service} onClose={closeOperation} onSaved={finishOperation} />
      )}
      {operation?.kind === "source" && (
        <AppSourceDialog app={operation.app} locale={locale} service={service} onClose={closeOperation} onSaved={finishOperation} />
      )}
      {operation?.kind === "publish" && (
        <AppPublishDialog app={operation.app} locale={locale} service={service} onClose={closeOperation} onSaved={finishOperation} />
      )}
    </section>
  )
}
