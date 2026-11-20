/**
 * AppFactoryPanel — 「应用工厂」：就绪度、卸载、再创作来源、环境提升历史。
 *
 * 四块行为共用一个面板，因为它们是同一份应用的生命周期视图：
 *
 * - **就绪度**是派生读模型（六个环节），刷新即可重算，不存储；
 * - **卸载**是前向命令（先停用 IAM 安装再归档），不可回退；
 * - **再创作**开关控制是否允许别人下载源代码再创作；
 * - **提升历史**是环境的不可变发布链，审批门禁在这里体现为审批人留痕。
 */
import { useCallback, useEffect, useMemo, useState } from "react";
import type { AppResponse, FactoryReadinessCheck, SdkworkDeployAppClient } from "@sdkwork/deployments-pc-console-core/sdk";
import type { DeploymentsLocale } from "@sdkwork/deployments-pc-commons";
import { publishingTranslator } from "../i18n.ts";
import {
  createAppFactoryService,
  type AppFactoryService,
  type AppFactoryState,
  type PromotionHistoryPage,
} from "../service/app-factory.ts";
import css from "./app-factory.module.css";

export interface AppFactoryPanelProps {
  readonly deployClient: SdkworkDeployAppClient
  readonly app: AppResponse
  readonly locale: DeploymentsLocale
  readonly service?: AppFactoryService | undefined
}

function messageOf(cause: unknown): string {
  return cause instanceof Error ? cause.message : String(cause);
}

function statusClass(status: string): string {
  if (status === "READY") return css.badgeReady;
  if (status === "BLOCKED") return css.badgeBlocked;
  return css.badgePending;
}

export function AppFactoryPanel({ deployClient, app, locale, service: injected }: AppFactoryPanelProps) {
  const t = useMemo(() => publishingTranslator(locale), [locale]);
  const service = useMemo(
    () => injected ?? createAppFactoryService({ deployClient }),
    [injected, deployClient],
  );

  const [state, setState] = useState<AppFactoryState>();
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>();
  const [environmentId, setEnvironmentId] = useState("");
  const [promotions, setPromotions] = useState<PromotionHistoryPage>();
  const [uninstallStatus, setUninstallStatus] = useState<string>();
  const [form, setForm] = useState({ releaseId: "", approve: false, approverUserId: "", note: "" });

  const load = useCallback(async () => {
    setLoading(true);
    setError(undefined);
    try {
      const loaded = await service.load(app.id);
      setState(loaded);
      setEnvironmentId((current) => current || loaded.environments[0]?.id || "");
    } catch (cause) {
      setError(messageOf(cause));
    } finally {
      setLoading(false);
    }
  }, [service, app.id]);

  useEffect(() => {
    void load();
  }, [load]);

  useEffect(() => {
    if (!environmentId) return;
    let active = true;
    void service.listPromotions(app.id, environmentId).then((page) => {
      if (active) setPromotions(page);
    }).catch((cause) => {
      if (active) setError(messageOf(cause));
    });
    return () => { active = false };
  }, [service, app.id, environmentId]);

  async function handleUninstall() {
    if (!window.confirm(t("factoryUninstallConfirm"))) return;
    setBusy(true);
    setError(undefined);
    try {
      const result = await service.uninstall(app.id);
      setUninstallStatus(result.uninstallStatus);
      await load();
    } catch (cause) {
      setError(messageOf(cause));
    } finally {
      setBusy(false);
    }
  }

  async function handleRemix(enabled: boolean) {
    setBusy(true);
    setError(undefined);
    try {
      await service.setRemixEnabled(app.id, enabled);
      await load();
    } catch (cause) {
      setError(messageOf(cause));
    } finally {
      setBusy(false);
    }
  }

  async function handlePromote(event: React.FormEvent) {
    event.preventDefault();
    if (!environmentId || form.releaseId.trim() === "") return;
    setBusy(true);
    setError(undefined);
    try {
      await service.promote(app.id, environmentId, {
        releaseId: form.releaseId.trim(),
        approve: form.approve,
        ...(form.approverUserId.trim() === "" ? {} : { approverUserId: form.approverUserId.trim() }),
        ...(form.note.trim() === "" ? {} : { note: form.note.trim() }),
      });
      setForm({ releaseId: "", approve: false, approverUserId: "", note: "" });
      await load();
      setPromotions(await service.listPromotions(app.id, environmentId));
    } catch (cause) {
      setError(messageOf(cause));
    } finally {
      setBusy(false);
    }
  }

  return (
    <section className={css.panel} aria-label={t("factoryTitle")}>
      <header className={css.header}>
        <h3 className={css.title}>{t("factoryTitle")}</h3>
        <button type="button" className={css.button} onClick={() => void load()} disabled={loading || busy}>
          {t("factoryRefresh")}
        </button>
      </header>

      {loading ? <p className={css.muted}>{t("factoryLoading")}</p> : null}
      {error ? <p className={css.error} role="alert">{error}</p> : null}

      {state ? (
        <>
          <section className={css.section} aria-label={t("factoryReadiness")}>
            <h4 className={css.sectionTitle}>{t("factoryReadiness")}</h4>
            <p className={css.status}>
              <span className={`${css.badge} ${statusClass(state.readiness.status)}`}>
                {state.readiness.status}
              </span>
              {state.readiness.blockingStep ? (
                <span className={css.muted}>
                  {t("factoryBlockingStep", { step: state.readiness.blockingStep })}
                </span>
              ) : null}
            </p>
            <ul className={css.stages}>
              {state.readiness.checks.map((check: FactoryReadinessCheck) => (
                <li key={check.step} className={css.stage}>
                  <span>{check.step}</span>
                  <span className={`${css.badge} ${statusClass(check.status)}`}>{check.status}</span>
                  <span className={css.muted}>{check.reason ?? check.detail ?? ""}</span>
                </li>
              ))}
            </ul>
            <div className={css.actions}>
              <button
                type="button"
                className={`${css.button} ${css.buttonDanger}`}
                onClick={() => void handleUninstall()}
                disabled={busy || app.appStatus === "ARCHIVED"}
              >
                {t("factoryUninstall")}
              </button>
              {uninstallStatus ? (
                <span className={css.muted}>{t("factoryUninstalled", { status: uninstallStatus })}</span>
              ) : null}
            </div>
          </section>

          <section className={css.section} aria-label={t("factoryRemix")}>
            <h4 className={css.sectionTitle}>{t("factoryRemix")}</h4>
            <p className={css.status}>
              <span className={`${css.badge} ${state.remix.remixEnabled ? css.badgeReady : css.badgePending}`}>
                {state.remix.remixEnabled ? "ON" : "OFF"}
              </span>
              <span className={css.muted}>
                {state.remix.readiness}
                {state.remix.readinessReason ? ` · ${state.remix.readinessReason}` : ""}
              </span>
            </p>
            <div className={css.actions}>
              <button
                type="button"
                className={css.button}
                onClick={() => void handleRemix(!state.remix.remixEnabled)}
                disabled={busy}
              >
                {state.remix.remixEnabled ? t("factoryRemixDisable") : t("factoryRemixEnable")}
              </button>
            </div>
          </section>

          <section className={css.section} aria-label={t("factoryPromotions")}>
            <h4 className={css.sectionTitle}>{t("factoryPromotions")}</h4>
            <label className={css.field}>
              <span>{t("factoryEnvironment")}</span>
              <select
                value={environmentId}
                onChange={(event) => setEnvironmentId(event.target.value)}
              >
                {state.environments.map((environment) => (
                  <option key={environment.id} value={environment.id}>
                    {environment.envKey} · {environment.envStatus}
                  </option>
                ))}
              </select>
            </label>
            {promotions && promotions.items.length > 0 ? (
              <table className={css.table}>
                <thead>
                  <tr>
                    <th>release</th>
                    <th>status</th>
                    <th>approver</th>
                    <th>createdAt</th>
                  </tr>
                </thead>
                <tbody>
                  {promotions.items.map((promotion) => (
                    <tr key={promotion.id}>
                      <td>{promotion.releaseVersion || promotion.releaseId}</td>
                      <td>{promotion.environmentKey}</td>
                      <td>{promotion.approverUserId ?? "—"}</td>
                      <td>{promotion.createdAt}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            ) : (
              <p className={css.muted}>{t("factoryEmptyPromotions")}</p>
            )}
            <form className={css.form} onSubmit={(event) => void handlePromote(event)}>
              <label className={css.field}>
                <span>{t("factoryReleaseId")}</span>
                <input
                  value={form.releaseId}
                  onChange={(event) => setForm({ ...form, releaseId: event.target.value })}
                  required
                />
              </label>
              <label className={css.field}>
                <span>{t("factoryApproverUserId")}</span>
                <input
                  value={form.approverUserId}
                  onChange={(event) => setForm({ ...form, approverUserId: event.target.value })}
                />
              </label>
              <label className={css.field}>
                <span>{t("factoryNote")}</span>
                <input
                  value={form.note}
                  onChange={(event) => setForm({ ...form, note: event.target.value })}
                />
              </label>
              <label className={css.checkbox}>
                <input
                  type="checkbox"
                  checked={form.approve}
                  onChange={(event) => setForm({ ...form, approve: event.target.checked })}
                />
                <span>{t("factoryApprove")}</span>
              </label>
              <div className={css.actions}>
                <button type="submit" className={css.button} disabled={busy}>
                  {t("factoryPromote")}
                </button>
              </div>
            </form>
          </section>
        </>
      ) : null}
    </section>
  );
}
