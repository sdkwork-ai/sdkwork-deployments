/**
 * SourceSpecsAdminPage — backend-admin 面上的「源码规格」运维视图。
 *
 * ## 它回答的问题
 *
 * 控制台的「源码规格」抽屉一次只看一个应用；运维要问的是**跨应用**的那一版：
 * 「这个环境里，有哪些应用的某类客户端其实没有可服务的源码」。所以本页把同一批
 * 事实摊成一张台账 —— 应用 → 规格 → 每端（默认 / 回退 / 是否真在服务）。
 *
 * ## 三个刻意的取舍
 *
 * - **只读。** 声明规格、改某端默认、绑定来源都需要应用面的写权限，而
 *   backend-admin 的 SDK 根本没有源码规格面（见 `port.tsx`）。本页因此只呈现
 *   结果，并在页脚明说写操作在哪 —— 「这个页面上有一个按不动的按钮」比「没有
 *   按钮但没人解释」更糟。
 * - **一次只读一页应用，页大小由端口定（10）。** 契约没有「列出全部规格」的端点，
 *   `sourceSpecs.list` 是**按应用**的，所以这一页必然是 N+1 次请求。页大小留在
 *   端口而不在这里，是因为**成本属于发请求的那一侧**：本页只往上/往下翻一页
 *   （`hasMore` 由服务端给），从不自己决定一次要多少个应用，所以这里不该有第二个
 *   数字去和端口「保持一致」。`admin-local-projects` 已经证明 admin 面可以有自己的
 *   读取粒度。
 * - **口径文案不自带。** 客户端类别、运行目标、处理方式这些词表属于发布能力，
 *   由适配器随行一起下发（见 `port.tsx` 的说明）—— 在这里再抄一份，契约加一个
 *   `runtimeTarget` 时两个面就会显示不同的东西。
 */
import { useCallback, useEffect, useMemo, useState } from "react";
import type { DeploymentsLocale } from "@sdkwork/deployments-pc-commons";
import { translateSourceSpecs, type SourceSpecsMessageKey } from "./i18n.ts";
import {
  useSourceSpecsAdminPort,
  type SourceSpecsAdminPage as SourceSpecsAdminLedger,
  type SourceSpecsAdminSpec,
} from "./port.tsx";
import "./SourceSpecsAdminPage.css";

export interface SourceSpecsAdminPageProps {
  readonly locale: DeploymentsLocale
}

type Translator = (key: SourceSpecsMessageKey, values?: Record<string, string | number>) => string;

export function SourceSpecsAdminPage({ locale }: SourceSpecsAdminPageProps) {
  const t = useMemo<Translator>(
    () => (key, values) => translateSourceSpecs(locale, key, values),
    [locale],
  );
  const port = useSourceSpecsAdminPort();

  /**
   * The operator's pick, or `""` while they have not made one.
   *
   * Derived rather than copied into state by an effect: the port arrives with the
   * host, so an initial `useState(port?.defaultEnvironment)` would freeze the
   * choice at mount and never follow a port that mounts later.
   */
  const [pickedEnvironment, setPickedEnvironment] = useState("");
  const environment = pickedEnvironment === "" ? port?.defaultEnvironment ?? "" : pickedEnvironment;

  const [page, setPage] = useState(1);
  const [ledger, setLedger] = useState<SourceSpecsAdminLedger>()
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>();
  const [refresh, setRefresh] = useState(0);

  useEffect(() => {
    if (port === null || environment === "") return;
    let active = true;
    setBusy(true);
    setError(undefined);
    void port.list({ page, environment }).then((result) => {
      // 读失败时不保留上一页的内容：跨应用台账里「上一条租户的数据」和
      // 「当前租户没有数据」看起来一样，而后者才是真相。
      if (active) setLedger(result);
    }).catch((cause) => {
      if (active) {
        setLedger(undefined);
        setError(t("error.load", { message: messageOf(cause) }));
      }
    }).finally(() => {
      if (active) setBusy(false);
    });
    return () => { active = false };
  }, [environment, page, port, refresh, t]);

  const reload = useCallback(() => {
    setLedger(undefined);
    setRefresh((value) => value + 1);
  }, []);

  const totals = useMemo(() => {
    const specs = (ledger?.applications ?? []).flatMap((application) => application.specs);
    return {
      applications: ledger?.applications.length ?? 0,
      specs: specs.length,
      bound: specs.filter((spec) => !spec.silent).length,
      silent: specs.filter((spec) => spec.silent).length,
    };
  }, [ledger]);

  return (
    <section className="source-specs-page resource-page">
      <header className="page-header">
        <div>
          <span className="eyebrow">{t("page.eyebrow")}</span>
          <h1>{t("page.title")}</h1>
          <p>{t("page.description")}</p>
        </div>
        <div className="actions">
          <label className="source-specs-field">
            <span>{t("toolbar.environment")}</span>
            <select
              aria-label={t("toolbar.environment")}
              value={environment}
              disabled={busy || port === null}
              onChange={(event) => { setPickedEnvironment(event.target.value); setPage(1); }}
            >
              {(port?.environments ?? []).map((value) => (
                <option key={value} value={value}>{value}</option>
              ))}
            </select>
          </label>
          <button type="button" className="command-button" disabled={busy || port === null} onClick={reload}>
            {t("toolbar.refresh")}
          </button>
        </div>
      </header>

      {port === null && (
        <div className="error-banner" role="alert">{t("error.noPort")}</div>
      )}
      {error !== undefined && (
        <div className="error-banner" role="alert">{error}</div>
      )}
      {ledger !== undefined && ledger.unreadableApplications.length > 0 && (
        <div className="error-banner" role="alert">
          {t("error.partial", { apps: ledger.unreadableApplications.join(", ") })}
        </div>
      )}

      {busy && <div className="source-specs-status" role="status">{t("toolbar.loading")}</div>}

      <div className="source-specs-summary">
        <Summary label={t("summary.applications")} value={totals.applications} />
        <Summary label={t("summary.specs")} value={totals.specs} />
        <Summary label={t("summary.bound")} value={totals.bound} />
        {/* 「不产出规则」单独立一格：它不是错误，但它是这一页存在的理由 ——
            一个绑了来源却没声明服务哪一端的规格，和一份没有来源的规格，症状
            都是「访问它不生效」。 */}
        <Summary label={t("summary.silent")} value={totals.silent} tone={totals.silent > 0 ? "warn" : "plain"} />
      </div>

      {ledger !== undefined && ledger.applications.length === 0 && (
        <div className="empty-state">{t("empty")}</div>
      )}

      <div className="source-specs-ledger">
        {(ledger?.applications ?? []).map((application) => (
          <section key={application.appId} className="source-specs-app" aria-label={application.appName}>
            <header className="source-specs-app-header">
              <strong>{application.appName}</strong>
              <code>{application.appId}</code>
              <span className="status-badge">{application.appStatusLabel}</span>
              <span className="source-specs-count">
                {t("app.specCount", { count: application.specs.length })}
              </span>
            </header>
            {application.specs.length === 0
              ? <div className="empty-state">{t("app.noSpecs")}</div>
              : (
                <ul className="source-specs-list">
                  {application.specs.map((spec) => (
                    <SpecRow key={spec.specId} spec={spec} t={t} />
                  ))}
                </ul>
              )}
          </section>
        ))}
      </div>

      <footer className="source-specs-footer">
        <button
          type="button"
          className="command-button"
          disabled={busy || page <= 1}
          onClick={() => { setPage((value) => Math.max(1, value - 1)); }}
        >
          {t("toolbar.prev")}
        </button>
        <span>{t("toolbar.page", { page })}</span>
        <button
          type="button"
          className="command-button"
          disabled={busy || ledger?.hasMore !== true}
          onClick={() => { setPage((value) => value + 1); }}
        >
          {t("toolbar.next")}
        </button>
      </footer>

      <aside className="source-specs-readonly">
        <strong>{t("readOnly.title")}</strong>
        <span>{t("readOnly.note")}</span>
      </aside>
    </section>
  );
}

function Summary({ label, value, tone = "plain" }: { label: string; value: number; tone?: "plain" | "warn" }) {
  return (
    <div className="source-specs-metric" data-tone={tone}>
      <span className="source-specs-metric-value">{value}</span>
      <span className="source-specs-metric-label">{label}</span>
    </div>
  );
}

/**
 * One source spec, with its per-client ranks.
 *
 * The chips carry both facts the operator needs to tell apart: the rank that was
 * *declared* and whether this source actually *serves* it. Showing only one of
 * them is how a panel ends up confidently naming a source that answers nothing.
 */
function SpecRow({ spec, t }: { spec: SourceSpecsAdminSpec; t: Translator }) {
  return (
    <li className="source-specs-item">
      <div className="source-specs-item-main">
        <span className="source-specs-item-title">
          <strong>{spec.label}</strong>
          <code>{spec.specKey}</code>
          {spec.isAppDefault && <span className="source-specs-chip">{t("spec.appDefault")}</span>}
        </span>
        <span className="source-specs-item-meta">
          <span className="source-specs-chip">{spec.runtimeTargetLabel}</span>
          <span className="source-specs-chip">{spec.clientArchitectureLabel}</span>
          <span className="source-specs-chip">{spec.handlerLabel}</span>
          <span className="source-specs-chip" data-tone={spec.disabled ? "danger" : "plain"}>
            {spec.statusLabel}
          </span>
          <span className="source-specs-chip" data-tone={spec.silent ? "warn" : "ok"}>
            {spec.silent ? t("spec.empty") : t("spec.bound")}
          </span>
          {spec.disabled && <span className="source-specs-chip" data-tone="danger">{t("spec.disabled")}</span>}
        </span>
        <span className="source-specs-routes">
          {spec.routes.length === 0
            ? <span className="source-specs-route" data-tone="warn">{t("route.none")}</span>
            : spec.routes.map((route) => (
              <span
                key={`${route.clientClass}#${route.preference}`}
                className="source-specs-route"
                data-answers={route.isEffective}
                data-serving={route.serves}
              >
                {route.clientClassLabel}
                <em>
                  {route.isDeclaredDefault
                    ? t("route.declaredDefault")
                    : t("route.fallback", { rank: route.preference })}
                </em>
                {/*
                 * Exactly one status marker per route. The three cases cannot
                 * overlap, and that is a property of the kernel rather than of
                 * this expression: `describeClientClassRouting` picks `effective`
                 * as the lowest rank that *serves*, so `isEffective` implies
                 * `serves`. Naming all three is what keeps a live fallback
                 * legible — with only the two outer cases it renders bare, and a
                 * dashed border is the only thing telling it apart from a rank
                 * that produces no rule at all. Telling "declared" from
                 * "actually serving" is why this page exists.
                 */}
                <em>
                  {route.isEffective
                    ? t("route.answers")
                    : route.serves
                      ? t("route.serves")
                      : t("route.dark")}
                </em>
              </span>
            ))}
        </span>
      </div>
    </li>
  );
}

function messageOf(cause: unknown): string {
  return cause instanceof Error && cause.message ? cause.message : String(cause);
}
