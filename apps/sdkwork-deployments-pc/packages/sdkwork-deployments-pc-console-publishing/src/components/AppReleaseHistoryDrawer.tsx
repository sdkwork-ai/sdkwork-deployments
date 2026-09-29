/**
 * AppReleaseHistoryDrawer — 应用行上的「发布历史」命令。
 *
 * ## 为什么是独立抽屉，而不是详情里再加一段
 *
 * 「详情」回答的是**这个应用现在是什么**（身份、域名、平台目标、来源、生命周期），
 * 是配置的横截面；「发布历史」回答的是**它一路上被发布成什么样**，是时间的纵轴。
 * 把后者塞进前者，会得到一个既有静态字段又有时间线的长页面，而那正是运维要对照
 * 「现在这一版和上一版差了哪一次操作」时最不该有的形态。
 *
 * ## 一条记录是怎么来的
 *
 * 契约里三段是分开读的：`packages.list` / `releases.list` / `deployments.list`。
 * 合并与「能不能回滚」的判定都在 `service/app-release-history.ts` 里，是纯函数 ——
 * 这个组件只负责把它算出来的东西摆上去，并在操作员按下回滚时把请求送出去。
 * 这么切分不是为了好看：那两条判断（哪一版在线上、回滚要抄哪一次部署的参数）
 * 是这个能力真正的领域逻辑，必须能被变异测试证明真的在拦人，而不是只测出「渲染不崩」。
 *
 * ## 回滚是写操作，所以它有前置条件
 *
 * 契约**没有**回滚端点：回滚 = 以历史 release 再发一次 `deployments.create`，而那
 * 四个目标参数客户端推不出来、只能从既有部署记录抄。{@link rollbackAvailability}
 * 把「抄不到」拆成五种有名字的原因，于是按钮要么可点、要么旁边就写着为什么 ——
 * 没有第三种「点了才报错」的形态。按下之后还有一道确认：它是一次发布，不是撤销。
 */
import { useEffect, useMemo, useState } from "react";
import type { AppResponse, ReleaseStatus, SdkworkDeployAppClient } from "@sdkwork/deployments-pc-console-core/sdk";
import type { SdkworkDriveAppClient } from "@sdkwork/deployments-pc-console-core/sdk";
import type { DeploymentsLocale } from "@sdkwork/deployments-pc-commons";
import { RotateCcw } from "lucide-react";
import {
  publishingTranslator,
  DEPLOYMENT_KIND_LABEL_KEYS,
  DEPLOYMENT_STATUS_LABEL_KEYS,
  DEPLOYMENT_TARGET_LABEL_KEYS,
  PACKAGE_FORMAT_LABEL_KEYS,
  PACKAGE_STATUS_LABEL_KEYS,
  RELEASE_STATUS_LABEL_KEYS,
  type PublishingMessageKey,
  type PublishingTranslator,
} from "../i18n.ts";
import {
  createDeployAppOperationsService,
  type DeployAppOperationsService,
  type DeployAppReleaseHistorySnapshot,
} from "../service/deploy-app-operations.ts";
import {
  deploymentStatusTone,
  formatByteSize,
  formatHistoryTime,
  packageStatusTone,
  RELEASE_HISTORY_PAGE_SIZE,
  releaseStatusTone,
  rollbackAvailability,
  shortDigest,
  type DeployAppReleaseHistoryEntry,
  type DeployAppRollbackBlockReason,
  type DeployAppRollbackPlan,
  type DeployStatusTone,
} from "../service/app-release-history.ts";
import css from "./create-deploy-app.module.css";

export interface AppReleaseHistoryDrawerProps {
  readonly deployClient: SdkworkDeployAppClient
  readonly driveClient: SdkworkDriveAppClient
  readonly locale: DeploymentsLocale
  /** 列表行已有的数据：标题先渲染，历史到达后补齐。 */
  readonly app: AppResponse
  readonly onClose: () => void
  readonly theme?: ("light" | "dark") | undefined
  readonly service?: DeployAppOperationsService | undefined
}

/**
 * 色调 → 模块样式类。
 *
 * 用本视图自己的四个令牌化胶囊类，而不是抽屉里既有的 `.statusOk` 系列：那四个把
 * 颜色写死在浅色值上（`rgba(22,163,74,.12)` 底 + `#16a34a` 字），深色主题下底几乎
 * 透明、对比度掉到读不清。历史是一个会在深色控制台里长时间停留的审计面，所以这
 * 四个色调走 `--pda-tone-*` 令牌，浅深两套值在同一处登记（见模块样式里的两段）。
 */
const TONE_CLASS: Readonly<Record<DeployStatusTone, string>> = {
  ok: css.historyToneOk,
  warn: css.historyToneWarn,
  bad: css.historyToneBad,
  neutral: css.historyToneNeutral,
};

/**
 * 不能回滚的五种原因 → 文案键。
 *
 * `Record` 而不是 `Partial`：`DeployAppRollbackBlockReason` 新增一种原因时这里
 * 必编译错，于是文案与判据一起补齐。回退成「当前不可回滚」会让操作员去猜，
 * 而这五种里至少三种各自需要完全不同的下一步动作。
 */
const ROLLBACK_REASON_KEYS: Readonly<Record<DeployAppRollbackBlockReason, PublishingMessageKey>> = {
  serving: "historyRollbackReasonServing",
  inFlight: "historyRollbackReasonInFlight",
  releaseNotDeployable: "historyRollbackReasonReleaseNotDeployable",
  packageNotDeployable: "historyRollbackReasonPackageNotDeployable",
  noReferenceDeployment: "historyRollbackReasonNoReference",
};

/** 发起回滚时要一起带来的展示上下文（确认框要说清「回滚到哪一版」）。 */
interface PendingRollback {
  readonly plan: DeployAppRollbackPlan
  readonly version: string
  readonly releaseStatus: ReleaseStatus
}

export function AppReleaseHistoryDrawer({
  deployClient,
  driveClient,
  locale,
  app,
  onClose,
  theme = "light",
  service: injectedService,
}: AppReleaseHistoryDrawerProps) {
  const t = useMemo(() => publishingTranslator(locale), [locale])
  const service = useMemo(
    () => injectedService ?? createDeployAppOperationsService({ deployClient, driveClient }),
    [injectedService, deployClient, driveClient],
  )

  const [history, setHistory] = useState<DeployAppReleaseHistorySnapshot>()
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string>()
  const [notice, setNotice] = useState<string>()
  /** 已选中的回滚目标；`undefined` = 没有待确认的回滚。 */
  const [pending, setPending] = useState<PendingRollback>()
  const [rolling, setRolling] = useState(false)
  /** 每次成功回滚后自增，重新拉一次历史（新部署要出现在时间线里）。 */
  const [reload, setReload] = useState(0)

  useEffect(() => {
    let active = true
    setLoading(true)
    void service.loadReleaseHistory(app.id).then((loaded) => {
      if (active) setHistory(loaded)
    }).catch((cause) => {
      if (active) setError(t("historyLoadFailed", { message: messageOf(cause) }))
    }).finally(() => {
      if (active) setLoading(false)
    })
    return () => { active = false }
  }, [app.id, service, t, reload])

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return
      // 待确认的回滚先撤销，再按一次才关框 —— 一个 Escape 同时撤掉确认框和整个
      // 抽屉，会让误触的代价从「重按一次」变成「重新打开并重新找那一版」。
      if (pending !== undefined) {
        setPending(undefined)
      } else {
        onClose()
      }
    }
    document.addEventListener("keydown", onKeyDown)
    const previousOverflow = document.body.style.overflow
    document.body.style.overflow = "hidden"
    return () => {
      document.removeEventListener("keydown", onKeyDown)
      document.body.style.overflow = previousOverflow
    }
  }, [onClose, pending])

  const confirmRollback = () => {
    if (pending === undefined) return
    setRolling(true)
    setError(undefined)
    void service.rollbackRelease(app.id, pending.plan).then((deployment) => {
      setNotice(t("historyRollbackSubmitted", {
        deploymentId: deployment.id,
        status: t(DEPLOYMENT_STATUS_LABEL_KEYS[deployment.deploymentStatus]),
      }))
      setPending(undefined)
      setReload((value) => value + 1)
    }).catch((cause) => {
      setError(t("historyRollbackFailed", { message: messageOf(cause) }))
      setPending(undefined)
    }).finally(() => {
      setRolling(false)
    })
  }

  const entries = history?.entries ?? []
  const unavailable = history?.unavailableSections ?? []
  /** 三段都没读回来，且一段都没读到东西 —— 此时说「从没发布过」是错的。 */
  const allUnavailable = history !== undefined && unavailable.length === 3 && entries.length === 0

  return (
    <div
      className={css.drawerRoot}
      data-theme={theme}
      role="presentation"
      onMouseDown={(event) => { if (event.target === event.currentTarget) onClose() }}
    >
      <div
        className={`${css.drawerPanel}${` ${css.drawerPanelLg}`}`}
        role="dialog"
        aria-modal="true"
        aria-label={t("historyTitle")}
      >
        <header className={css.header}>
          <div className={css.headerText}>
            <h2>{app.name}</h2>
            <p>{t("historyDescription")}</p>
          </div>
          <button type="button" className={css.closeButton} title={t("close")} aria-label={t("close")} onClick={onClose}>
            ×
          </button>
        </header>

        <div className={css.body}>
          {error && <div className={css.errorBanner} role="alert">{error}</div>}
          {notice && <div className={css.successBanner} role="status">{notice}</div>}
          {loading && history === undefined && <span className={css.fieldHint}>{t("historyLoading")}</span>}

          {pending !== undefined && (
            // 确认块固定在正文首位：它是一次**发布**，参数必须先看清再按。文案里点名
            // 参数抄自哪一次部署 —— 「按哪次的配置回滚」是这次操作唯一不确定的地方。
            <div className={css.historyConfirm} role="group" aria-label={t("historyRollbackTitle", { version: pending.version })}>
              <p className={css.historyConfirmTitle}>
                <RotateCcw aria-hidden="true" size={14} />
                {t("historyRollbackTitle", { version: pending.version })}
              </p>
              <p className={css.historyConfirmHint}>{t("historyRollbackDescription")}</p>
              <dl className={css.historyConfirmParams}>
                <dt>{t("historyColumnVersion")}</dt>
                <dd><code>{pending.version}</code></dd>
                <dt>{t("historyColumnStatus")}</dt>
                <dd><code>{t(RELEASE_STATUS_LABEL_KEYS[pending.releaseStatus])}</code></dd>
                <dt>{t("historyColumnTarget")}</dt>
                <dd>
                  <code>
                    {t(DEPLOYMENT_TARGET_LABEL_KEYS[pending.plan.deploymentTarget])}
                    {" · "}
                    {t(DEPLOYMENT_KIND_LABEL_KEYS[pending.plan.deploymentKind])}
                  </code>
                </dd>
                <dt>{t("historyColumnEnvironment")}</dt>
                <dd><code>{pending.plan.environment}</code></dd>
              </dl>
              <p className={css.historyConfirmHint}>
                {pending.plan.referenceIsServing
                  ? t("historyRollbackReferenceServing", { id: pending.plan.referenceDeploymentId })
                  : t("historyRollbackReferenceHistoric", { id: pending.plan.referenceDeploymentId })}
              </p>
              <div className={css.historyConfirmActions}>
                <button type="button" className={css.secondaryButton} disabled={rolling} onClick={() => { setPending(undefined) }}>
                  {t("cancel")}
                </button>
                <button type="button" className={css.historyDangerButton} disabled={rolling} onClick={confirmRollback}>
                  {rolling ? t("historyRollingBack") : t("historyRollbackConfirm")}
                </button>
              </div>
            </div>
          )}

          {/* ---------- 当前线上 ---------- */}
          <section className={css.detailSection}>
            <h3 className={css.detailSectionTitle}>{t("historySectionCurrent")}</h3>
            {history === undefined
              ? <span className={css.fieldHint}>{t("historyLoading")}</span>
              : history.servingDeployment === undefined
                ? (
                  <span className={css.fieldHint}>
                    {t(unavailable.includes("deployments") ? "historySectionUnavailable" : "historyCurrentNone")}
                  </span>
                )
                : (
                  <div className={css.historyCurrentCard}>
                    <div className={css.historyEntryHead}>
                      <strong className={css.historyVersion}>
                        {history.entries.find((entry) => entry.release.id === history.currentReleaseId)?.release.semanticVersion
                          ?? t("detailNoValue")}
                      </strong>
                      <span className={toneClass(deploymentStatusTone(history.servingDeployment.deploymentStatus))}>
                        {t(DEPLOYMENT_STATUS_LABEL_KEYS[history.servingDeployment.deploymentStatus])}
                      </span>
                      <span className={css.historyToneNeutral}>{history.servingDeployment.environment}</span>
                    </div>
                    <p className={css.historyEntryMeta}>
                      {t("historyCurrentSince", { time: formatHistoryTime(history.servingDeployment.createdAt, locale) })}
                      {" · "}
                      {t(DEPLOYMENT_TARGET_LABEL_KEYS[history.servingDeployment.deploymentTarget ?? "WEB_NODE"])}
                    </p>
                  </div>
                )}
          </section>

          {/* ---------- 版本时间线 ---------- */}
          <section className={css.detailSection}>
            <h3 className={css.detailSectionTitle}>{t("historySectionTimeline")}</h3>
            {history !== undefined && history.truncated && (
              <p className={css.fieldHint}>{t("historyTruncated", { count: RELEASE_HISTORY_PAGE_SIZE })}</p>
            )}
            {history === undefined
              ? <span className={css.fieldHint}>{t("historyLoading")}</span>
              : entries.length === 0
                ? (
                  <span className={css.fieldHint}>
                    {t(allUnavailable ? "historySectionUnavailable" : "historyEmpty")}
                  </span>
                )
                : (
                  <div className={css.historyList}>
                    {entries.map((entry) => (
                      <HistoryEntryRow
                        key={entry.release.id}
                        entry={entry}
                        history={history}
                        locale={locale}
                        t={t}
                        isCurrent={history.currentReleaseId === entry.release.id}
                        onRollback={(plan) => {
                          setNotice(undefined)
                          setError(undefined)
                          setPending({ plan, version: entry.release.semanticVersion, releaseStatus: entry.release.releaseStatus })
                        }}
                      />
                    ))}
                  </div>
                )}
          </section>

          {/* ---------- 无归属部署 ---------- */}
          {history !== undefined && history.orphanDeployments.length > 0 && (
            <section className={css.detailSection}>
              <h3 className={css.detailSectionTitle}>{t("historySectionOrphans")}</h3>
              <p className={css.fieldHint}>{t("historyOrphanHint")}</p>
              <div className={css.historyList}>
                {history.orphanDeployments.map((deployment) => (
                  <div key={deployment.id} className={css.historyEntry}>
                    <div className={css.historyEntryHead}>
                      <code className={css.historyOrphanId}>{deployment.id}</code>
                      <span className={toneClass(deploymentStatusTone(deployment.deploymentStatus))}>
                        {t(DEPLOYMENT_STATUS_LABEL_KEYS[deployment.deploymentStatus])}
                      </span>
                      <span className={css.historyToneNeutral}>{deployment.environment}</span>
                    </div>
                    <p className={css.historyEntryMeta}>
                      {formatHistoryTime(deployment.createdAt, locale)}
                    </p>
                  </div>
                ))}
              </div>
            </section>
          )}
        </div>

        <footer className={css.footer}>
          <span className={css.footerSpacer} />
          <button type="button" className={css.secondaryButton} onClick={onClose}>
            {t("close")}
          </button>
        </footer>
      </div>
    </div>
  )
}

/* ------------------------------------------------------------------ *
 * Sub-components
 * ------------------------------------------------------------------ */

function HistoryEntryRow({
  entry,
  history,
  locale,
  t,
  isCurrent,
  onRollback,
}: {
  entry: DeployAppReleaseHistoryEntry
  history: DeployAppReleaseHistorySnapshot
  locale: DeploymentsLocale
  t: PublishingTranslator
  isCurrent: boolean
  onRollback: (plan: DeployAppRollbackPlan) => void
}) {
  const availability = rollbackAvailability(entry, history)
  const latestDeployment = entry.deployments[0]
  const pkg = entry.package

  return (
    // `data-current` 是**稳定的状态钩子**，与类名分开：CSS Module 会把类名哈希化，
    // 于是「哪一行是当前版本」在 DOM 上只剩这个属性可判 —— 样式、宿主门禁与渲染
    // 测试三处都读它，各自不必解析被改过的类名。
    <div className={`${css.historyEntry}${isCurrent ? ` ${css.historyEntryCurrent}` : ""}`} data-current={isCurrent}>
      <div className={css.historyEntryHead}>
        <strong className={css.historyVersion}>{entry.release.semanticVersion}</strong>
        <span className={css.historyBuild}>{t("historyBuildNumber", { build: entry.release.buildNumber })}</span>
        <span className={toneClass(releaseStatusTone(entry.release.releaseStatus))}>
          {t(RELEASE_STATUS_LABEL_KEYS[entry.release.releaseStatus])}
        </span>
        {isCurrent && <span className={css.historyToneNeutral}>{t("historyCurrentBadge")}</span>}
        {availability.kind === "available"
          ? (
            <button
              type="button"
              className={css.historyRollbackButton}
              title={t("historyRollbackAction")}
              aria-label={`${t("historyRollbackAction")} ${entry.release.semanticVersion}`}
              onClick={() => { onRollback(availability.plan) }}
            >
              <RotateCcw aria-hidden="true" size={13} />
              {t("historyRollbackAction")}
            </button>
          )
          : (
            // 不可回滚时**不是**把按钮置灰就完事：置灰只说「不行」，说不出为什么，
            // 更说不出下一步该做什么。这里直接换成原因本身。
            <span className={css.historyBlocked} title={t(ROLLBACK_REASON_KEYS[availability.reason])}>
              {t(ROLLBACK_REASON_KEYS[availability.reason], { status: blockedStatusText(availability.reason, entry, t) })}
            </span>
          )}
      </div>

      {/* 制品：三段里唯一描述「发布的是什么字节」的一段 */}
      <p className={css.historyEntryArtifact}>
        {pkg === undefined
          ? <span className={css.historyMuted}>{t("historyArtifactMissing")}</span>
          : (
            <>
              <span>{t(PACKAGE_FORMAT_LABEL_KEYS[pkg.packageFormat])}</span>
              {formatByteSize(pkg.packageSizeBytes) !== undefined && (
                <> {" · "} <span>{formatByteSize(pkg.packageSizeBytes)}</span></>
              )}
              {" · "}
              <span className={toneClass(packageStatusTone(pkg.packageStatus))}>
                {t(PACKAGE_STATUS_LABEL_KEYS[pkg.packageStatus])}
              </span>
              {" · "}
              <code>{t("historyArtifactDigest", { digest: shortDigest(pkg.checksumSha256) ?? "" })}</code>
            </>
          )}
      </p>

      {/* 部署：目标四元组 + 最近一次部署的落点。
          分支条件是 `latestDeployment === undefined` 而不是 `length === 0`：两者等价，
          但只有前者能让类型收窄，否则下面每一处访问都要再写一遍 `?.`。 */}
      <p className={css.historyEntryMeta}>
        {latestDeployment === undefined
          ? <span className={css.historyMuted}>{t("historyNoDeployment")}</span>
          : (
            <>
              <span>{t(DEPLOYMENT_TARGET_LABEL_KEYS[latestDeployment.deploymentTarget ?? "WEB_NODE"])}</span>
              {" · "}
              <span>{t(DEPLOYMENT_KIND_LABEL_KEYS[latestDeployment.deploymentKind ?? "ARTIFACT_RELEASE"])}</span>
              {" · "}
              <code>{latestDeployment.environment}</code>
              {" · "}
              <span className={toneClass(deploymentStatusTone(latestDeployment.deploymentStatus))}>
                {t(DEPLOYMENT_STATUS_LABEL_KEYS[latestDeployment.deploymentStatus])}
              </span>
              {" · "}
              <span>{t("historyDeploymentCount", { count: entry.deployments.length })}</span>
            </>
          )}
      </p>

      <p className={css.historyEntryMeta}>
        {t("historyColumnCutAt")}
        {" "}
        {formatHistoryTime(entry.release.createdAt, locale)}
        {latestDeployment !== undefined && (
          <>
            {" · "}
            {t("historyColumnLastDeployment")}
            {" "}
            {formatHistoryTime(latestDeployment.createdAt, locale)}
          </>
        )}
      </p>
    </div>
  )
}

/* ------------------------------------------------------------------ *
 * Helpers
 * ------------------------------------------------------------------ */

function toneClass(tone: DeployStatusTone): string {
  return TONE_CLASS[tone]
}

/**
 * 三种原因里有两句带 `{status}` 占位符，各取各自要说明的那个状态。
 *
 * 分开取名而不是传一个笼统的 status：`releaseNotDeployable` 要说的是**版本**的
 * 状态，`packageNotDeployable` 要说的是**制品**的状态，把版本状态塞进后者会得到
 * 一句技术上通顺、事实上错误的话。
 */
function blockedStatusText(
  reason: DeployAppRollbackBlockReason,
  entry: DeployAppReleaseHistoryEntry,
  t: PublishingTranslator,
): string {
  if (reason === "releaseNotDeployable") {
    return t(RELEASE_STATUS_LABEL_KEYS[entry.release.releaseStatus])
  }
  if (reason === "packageNotDeployable") {
    return t(PACKAGE_STATUS_LABEL_KEYS[entry.package?.packageStatus ?? "RETIRED"])
  }
  return ""
}

function messageOf(cause: unknown): string {
  return cause instanceof Error && cause.message ? cause.message : String(cause)
}
