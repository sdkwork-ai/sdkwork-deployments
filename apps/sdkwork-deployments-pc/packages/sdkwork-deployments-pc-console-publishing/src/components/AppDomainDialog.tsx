/**
 * AppDomainDialog — 应用行上的「域名设置」命令（行业标准双轨）。
 *
 * 参考行业 SaaS（Vercel / Netlify / Cloudflare Pages）的通行做法，域名分两类，
 * 本对话框一次覆盖两者。**自定义域名在上、平台预置域名在下** —— 用户真正要
 * 完成的事是「把我的域名指过来」，平台域名是后台自动开通的既成事实。
 *
 *   1. **自定义域名**（CUSTOM，置顶）—— 用户自有域名。**直接输入完整域名**，
 *      服务端是权威：本组件按最长后缀匹配已登记区域（`inferDomainZone`），
 *      匹配不到也允许提交，由服务端裁决。**最多 {@link MAX_CUSTOM_DOMAINS} 个**，
 *      支持动态添加与删除。
 *   2. **平台预置域名**（DEFAULT，次要）—— `appId.app.<suffix>`。每个应用都自动
 *      获得，覆盖全部 5 个生命周期环境（`app` / `app-dev` / `app-test`
 *      / `app-staging` / `app-demo`）。服务端在应用创建时已自动开通，这里只允许
 *      改 `appId`（`appDomainLabel`）与后缀目录（`appDomainSuffixes`），保存后
 *      服务端重新对账全部环境。
 *
 * 两条**不能想当然**的实现约束：
 *
 * - `apps.update` 把 `null` 当「清除覆盖」而不是「保持不变」，所以用户没动过的
 *   字段必须**整个字段缺席**（`undefined`），否则一次保存会把已有覆盖抹掉。
 * - 后缀目录**没有独立接口**。`AppResponse.appDomainSuffixes` 返回的已经是
 *   「有效值」（有覆盖用覆盖，否则平台目录），因此它同时充当默认值与当前值。
 *
 * 组件为纯 props 输入（生成式 client + locale），不依赖 console context。
 */
import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import type { AppResponse, DomainZoneResponse, SdkworkDeployAppClient } from "@sdkwork/deployments-pc-console-core/sdk";
import type { SdkworkDriveAppClient } from "@sdkwork/deployments-pc-console-core/sdk";
import type { DeploymentsLocale } from "@sdkwork/deployments-pc-commons";
import { publishingTranslator, type PublishingTranslator } from "../i18n.ts";
import {
  compositionKey,
  createDeployAppOperationsService,
  customDomainCapacity,
  domainStatusLabel,
  inferDomainZone,
  isHostnameShaped,
  MAX_CUSTOM_DOMAINS,
  normalizeHostname,
  type DeployAppDomain,
  type DeployAppDomainState,
  type DeployAppOperationsService,
} from "../service/deploy-app-operations.ts";
import css from "./create-deploy-app.module.css";

/**
 * How many custom domains one application may serve.
 *
 * Re-exported from the operations service so the cap has exactly one definition;
 * the dialog re-exports it to keep its existing public surface.
 */
export { MAX_CUSTOM_DOMAINS } from "../service/deploy-app-operations.ts"

export interface AppDomainDialogProps {
  readonly deployClient: SdkworkDeployAppClient
  readonly driveClient: SdkworkDriveAppClient
  readonly locale: DeploymentsLocale
  readonly app: AppResponse
  /** 保存成功后回调（宿主刷新列表以更新域名列）。 */
  readonly onChanged?: ((summary: string) => void) | undefined
  readonly onClose: () => void
  readonly theme?: ("light" | "dark") | undefined
  readonly size?: ("md" | "lg") | undefined
  readonly service?: DeployAppOperationsService | undefined
}

/** 生命周期环境顺序（与 Rust `app_domain_label` 的 5 环境一致）。 */
const ENVIRONMENT_ORDER = ["development", "test", "staging", "demo", "production"] as const

type DomainEnvironment = (typeof ENVIRONMENT_ORDER)[number]

/**
 * 「全部」页签的哨兵值。真实环境名不可能等于它，所以可以安全地放进同一个
 * `selectedEnvironment` 状态里，不需要第二个布尔量。
 */
const ALL_ENVIRONMENTS = "all"

type EnvironmentTab = DomainEnvironment | typeof ALL_ENVIRONMENTS

/**
 * 过滤出属于某个页签的域名。
 *
 * 「全部」返回原数组（不复制）；指定环境则只留该环境的行。**服务端的
 * `environment` 是自由字符串**，所以未知环境名不会出现在任何具体页签里，
 * 只能在「全部」下看到 —— 这比把它们静默塞进某个页签要诚实。
 */
export function filterDomainsByEnvironment<T extends { readonly environment: string }>(
  domains: readonly T[],
  tab: string,
): readonly T[] {
  if (tab === ALL_ENVIRONMENTS) return domains
  return domains.filter((domain) => domain.environment === tab)
}

/**
 * 按出现顺序收集域名实际使用到的环境，并保证已知环境排在前面。
 *
 * 用**实际存在的环境**而不是写死的 `ENVIRONMENT_ORDER` 建页签：平台域名只
 * 覆盖 5 个生命周期环境，但服务端可能返回别的名字，页签必须跟着数据走，
 * 否则会出现「点进去永远空白」的空页签。
 */
export function environmentTabsInUse(
  domains: readonly { readonly environment: string }[],
): readonly string[] {
  const seen: string[] = []
  for (const domain of domains) {
    if (!seen.includes(domain.environment)) seen.push(domain.environment)
  }
  const known = ENVIRONMENT_ORDER.filter((environment) => seen.includes(environment))
  const unknown = seen.filter((environment) => !(ENVIRONMENT_ORDER as readonly string[]).includes(environment))
  return [...known, ...unknown]
}

/** 一行待添加的自定义域名。`id` 稳定，保证删中间一项时输入焦点不跳。 */
interface CustomDraft {
  readonly id: number
  hostname: string
}

export function AppDomainDialog({
  deployClient,
  driveClient,
  locale,
  app,
  onChanged,
  onClose,
  theme = "light",
  size = "lg",
  service: injectedService,
}: AppDomainDialogProps) {
  const t = useMemo(() => publishingTranslator(locale), [locale])
  const service = useMemo(
    () => injectedService ?? createDeployAppOperationsService({ deployClient, driveClient }),
    [injectedService, deployClient, driveClient],
  )

  const [state, setState] = useState<DeployAppDomainState>()
  const [loading, setLoading] = useState(true)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string>()
  const [notice, setNotice] = useState<string>()

  // 平台域名配置。null 表示「清除覆盖、回落到 slug / 平台目录」。
  const [labelInput, setLabelInput] = useState("")
  const [suffixInput, setSuffixInput] = useState<string[]>([])
  const [environmentTab, setEnvironmentTab] = useState<EnvironmentTab>(ALL_ENVIRONMENTS)
  const labelTouched = useRef(false)
  const suffixesTouched = useRef(false)

  // 自定义域名：已生效的来自 `state`，这里是「待添加」草稿行。
  const [zones, setZones] = useState<readonly DomainZoneResponse[]>([])
  const [drafts, setDrafts] = useState<CustomDraft[]>([])
  const nextDraftId = useRef(1)

  const current = state?.app ?? app
  /** 有效后缀：有覆盖用覆盖，否则平台目录 —— 服务端已算好，这里只防缺省。 */
  const currentSuffixes = current.appDomainSuffixes ?? []

  const reload = async () => {
    setLoading(true)
    try {
      const loaded = await service.loadDomainState(app.id)
      setState(loaded)
      setLabelInput(loaded.app.appDomainLabel ?? loaded.app.slug)
      setSuffixInput([...(loaded.app.appDomainSuffixes ?? [])])
      labelTouched.current = false
      suffixesTouched.current = false
      setError(undefined)
    } catch (cause) {
      setError(t("domainsLoadFailed", { message: messageOf(cause) }))
    } finally {
      setLoading(false)
    }
  }

  useEffect(() => { void reload() }, [app.id])

  // 区域清单只用于「输入时提示落在哪个区域」，加载失败不阻塞自定义域名录入
  // （服务端才是权威，匹配不到也允许提交）。
  useEffect(() => {
    let cancelled = false
    void service.listDomainZones()
      .then((loaded) => { if (!cancelled) setZones(loaded) })
      .catch(() => { if (!cancelled) setZones([]) })
    return () => { cancelled = true }
  }, [service])

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape" && !busy) onClose()
    }
    document.addEventListener("keydown", onKeyDown)
    const previousOverflow = document.body.style.overflow
    document.body.style.overflow = "hidden"
    return () => {
      document.removeEventListener("keydown", onKeyDown)
      document.body.style.overflow = previousOverflow
    }
  }, [busy, onClose])

  /* ---------------- 平台域名：派生与校验 ---------------- */

  const defaultDomains = useMemo(
    () => (state?.domains ?? []).filter((domain) => domain.kind === "DEFAULT"),
    [state],
  )
  const customDomains = useMemo(
    () => (state?.domains ?? []).filter((domain) => domain.kind === "CUSTOM"),
    [state],
  )

  const labelValid = labelInput === "" || /^[a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?$/.test(labelInput)
  const suffixesValid = suffixInput.length > 0 && suffixInput.every(isValidSuffix)

  /** 客户端预览：`<label>.app[-<env>].<suffix>`。 */
  const previewHostnames = useMemo(() => {
    const label = labelInput !== "" ? labelInput : current.slug
    const out: { environment: DomainEnvironment; hostname: string }[] = []
    for (const suffix of suffixInput) {
      for (const environment of ENVIRONMENT_ORDER) {
        out.push({
          environment,
          hostname: `${label}.${environment === "production" ? "app" : `app-${environment}`}.${suffix}`,
        })
      }
    }
    return out
  }, [labelInput, suffixInput, current.slug])

  /** 预览里实际用到的环境（按生命周期顺序），用来建「生成的域名」页签。 */
  const previewEnvironments = useMemo(
    () => environmentTabsInUse(previewHostnames),
    [previewHostnames],
  )
  /** 当前页签下要展示的预览行。 */
  const visiblePreviewHostnames = useMemo(
    () => filterDomainsByEnvironment(previewHostnames, environmentTab),
    [previewHostnames, environmentTab],
  )

  /**
   * 改了应用标识 / 后缀后，当前页签可能已不存在（该环境没有预览行了）。
   *
   * 回落到「全部」而不是留在空页签上 —— 空页签会让操作者以为域名丢了。
   * 放在 effect 里而非渲染期改写 state，避免与 React 的渲染纯粹性冲突。
   */
  useEffect(() => {
    if (environmentTab === ALL_ENVIRONMENTS) return
    if (previewEnvironments.includes(environmentTab)) return
    setEnvironmentTab(ALL_ENVIRONMENTS)
  }, [previewEnvironments, environmentTab])

  const dirty = labelTouched.current || suffixesTouched.current

  const savePlatformDomains = async () => {
    setBusy(true)
    setError(undefined)
    setNotice(undefined)
    try {
      // 未触碰 = 字段缺席（`undefined`），服务端保持原值；清空 = 显式 `null`，
      // 服务端清除覆盖并回落到 slug / 平台目录。两者语义完全不同。
      const body: Parameters<DeployAppOperationsService["saveDomainConfig"]>[1] = {}
      if (labelTouched.current) body.appDomainLabel = labelInput === "" ? null : labelInput
      if (suffixesTouched.current) {
        body.appDomainSuffixes = suffixInput.length === 0 ? null : suffixInput
      }
      await service.saveDomainConfig(app.id, body)
      await reload()
      const summary = t("domainSaved")
      setNotice(summary)
      onChanged?.(summary)
    } catch (cause) {
      setError(isConflict(cause)
        ? t("domainAppIdConflict")
        : t("domainUpdateFailed", { message: messageOf(cause) }))
    } finally {
      setBusy(false)
    }
  }

  /* ---------------- 自定义域名：动态增删 ---------------- */

  /** 已生效 + 待添加的总数，用来卡 5 个上限并算剩余额度。 */
  const { used: customTotal, atCapacity } = customDomainCapacity(
    customDomains.length,
    drafts.length,
  )

  const addDraft = () => {
    if (atCapacity) return
    setDrafts((list) => [...list, { id: nextDraftId.current++, hostname: "" }])
  }

  const removeDraft = (id: number) => {
    setDrafts((list) => list.filter((draft) => draft.id !== id))
  }

  const updateDraft = (id: number, hostname: string) => {
    setDrafts((list) => list.map((draft) => (draft.id === id ? { ...draft, hostname } : draft)))
  }

  /** 一次提交全部草稿：任一失败即停，成功的部分保留（已生效的会出现在列表里）。 */
  const bindDrafts = async () => {
    const pending = drafts.filter((draft) => normalizeHostname(draft.hostname) !== "")
    if (pending.length === 0) return
    setBusy(true)
    setError(undefined)
    setNotice(undefined)
    const bound: string[] = []
    try {
      for (const draft of pending) {
        const hostname = normalizeHostname(draft.hostname)
        const result = await service.bindCustomHostname(app.id, { hostname })
        bound.push(result.hostname.hostname)
        // 逐个提交，让中途失败时已成功的部分立即落到列表里，而不是回滚掉。
        setDrafts((list) => list.filter((item) => item.id !== draft.id))
        await reload()
      }
      const summary = bound.length === 1
        ? t("domainBound", { hostname: bound[0] ?? "" })
        : t("domainBoundMany", { count: String(bound.length) })
      setNotice(summary)
      onChanged?.(summary)
    } catch (cause) {
      await reload()
      setError(isConflict(cause)
        ? t("domainBindConflict")
        : t("domainUpdateFailed", { message: messageOf(cause) }))
    } finally {
      setBusy(false)
    }
  }

  const draftRowsValid = drafts.length > 0
    && drafts.every((draft) => {
      const host = normalizeHostname(draft.hostname)
      return host === "" || isHostnameShaped(host)
    })
  const draftHasContent = drafts.some((draft) => normalizeHostname(draft.hostname) !== "")
  const canBind = !busy && draftHasContent && draftRowsValid

  /**
   * 解绑一个自定义域名：把它从组合里移除。
   *
   * 先绑定后解绑共用 `composition.update`（整份替换语义），所以这里必须把
   * **其余**绑定全部带上，否则解绑一个会连带清掉别的。主机名本身（`deploy_domain`
   * 行）保留，因为它可能被别的应用或别的路径复用。
   */
  const unbindCustom = async (domain: DeployAppDomain) => {
    if (domain.domainId === undefined) {
      setError(t("domainUnbindUnavailable"))
      return
    }
    setBusy(true)
    setError(undefined)
    setNotice(undefined)
    try {
      const all = state?.domains ?? []
      const appBefore = await service.loadDomainState(app.id)
      const bindings = all
        .filter((item) => item.domainId !== undefined && item.domainId !== domain.domainId)
        .map((item) => ({
          key: compositionKey(item.hostname, item.pathPrefix ?? "/"),
          domainId: item.domainId as string,
          pathPrefix: item.pathPrefix ?? "/",
          action: { type: "SERVE" as const },
        }))
      await service.replaceDomainBindings(app.id, {
        version: appBefore.app.version,
        bindings,
      })
      await reload()
      const summary = t("domainUnbound", { hostname: domain.hostname })
      setNotice(summary)
      onChanged?.(summary)
    } catch (cause) {
      setError(t("domainUpdateFailed", { message: messageOf(cause) }))
    } finally {
      setBusy(false)
    }
  }

  /**
   * 底部操作条左侧状态槽的内容与语气：一次只显示一条，
   * 优先级 **错误 > 结果 > 「有无未保存改动」提示**。
   *
   * 三态**共用同一个槽位**是「底部只有一行」的前提 —— 提示与结果各占一个槽，
   * 就会退回「两条横条」的旧观感（见 `.footer` 的样式注释）。
   */
  const footerTone: "error" | "success" | "hint" =
    error !== undefined ? "error" : notice !== undefined ? "success" : "hint"
  const footerText = error ?? notice ?? (dirty ? t("domainSaveHintDirty") : t("domainSaveHintClean"))

  return (
    <div
      className={css.drawerRoot}
      data-theme={theme}
      role="presentation"
      onMouseDown={(event) => { if (event.target === event.currentTarget && !busy) onClose() }}
    >
      <div
        className={`${css.drawerPanel}${size === "lg" ? ` ${css.drawerPanelLg}` : ""}`}
        role="dialog"
        aria-modal="true"
        aria-label={t("domainsTitle", { name: current.name })}
      >
        <header className={css.header}>
          <div className={css.headerText}>
            <h2>{t("domainsTitle", { name: current.name })}</h2>
            <p>{t("domainsDescription")}</p>
          </div>
          <button type="button" className={css.closeButton} title={t("close")} aria-label={t("close")} onClick={onClose}>
            ×
          </button>
        </header>

        <div className={css.body}>
          {loading && <span className={css.fieldHint}>{t("domainListLoading")}</span>}

          {!loading && (
            <>
              {/* ---------- 1. 自定义域名（置顶：这是用户真正要做的事） ---------- */}
              <div className={css.field}>
                <div className={css.sectionHead}>
                  <span className={css.stepTitle}>{t("domainSectionCustom")}</span>
                  <span
                    className={css.counterPill}
                    data-full={atCapacity ? "true" : "false"}
                    aria-label={t("domainCustomCounterLabel", {
                      used: String(customTotal),
                      max: String(MAX_CUSTOM_DOMAINS),
                    })}
                  >
                    {customTotal}/{MAX_CUSTOM_DOMAINS}
                  </span>
                </div>
                <span className={css.fieldHint}>{t("domainSectionCustomHint")}</span>
              </div>

              {/* 已生效的自定义域名 —— 每行可删除（从组合中解绑）。 */}
              {customDomains.length > 0 && (
                <div className={css.domainTable}>
                  <div className={css.domainTableHeadBar}>
                    <span className={css.domainTableHeadTitle}>{t("domainKindCustom")}</span>
                  </div>
                  {customDomains.map((domain) => (
                    <CustomDomainRow
                      key={`${domain.hostname}${domain.pathPrefix ?? ""}`}
                      domain={domain}
                      t={t}
                      busy={busy}
                      onRemove={() => { void unbindCustom(domain) }}
                    />
                  ))}
                </div>
              )}

              {/* 待添加草稿行 —— 自由输入，边输边给「落在哪个区域」的提示。 */}
              {drafts.map((draft) => (
                <CustomDomainDraftRow
                  key={draft.id}
                  draft={draft}
                  t={t}
                  busy={busy}
                  zones={zones}
                  onChange={(value) => { updateDraft(draft.id, value) }}
                  onRemove={() => { removeDraft(draft.id) }}
                />
              ))}

              {/* 添加域名 / 保存 都不在这里 —— 它们由底部的唯一操作条（footer）承载。
                  正文只负责「看」与「改」，动作全部留在视口内。 */}

              {/* CNAME 指引 —— 行业标准做法：让用户把自有域名别名到平台主机名。 */}
              {defaultDomains.length > 0 && (
                <div className={css.dnsGuide}>
                  <span className={css.dnsGuideTitle}>{t("domainCnameTitle")}</span>
                  <span className={css.fieldHint}>{t("domainCnameHint")}</span>
                  <dl className={css.dnsRecord}>
                    <dt>{t("domainCnameRecord")}</dt>
                    <dd><code>CNAME</code></dd>
                    <dt>{t("domainCnameTarget")}</dt>
                    <dd>
                      <code>{defaultDomains[0]?.hostname ?? t("domainNotConfigured")}</code>
                      <CopyButton value={defaultDomains[0]?.hostname ?? ""} t={t} />
                    </dd>
                  </dl>
                </div>
              )}

              {atCapacity && (
                <span className={css.fieldHint}>
                  {t("domainCustomLimitReached", { max: String(MAX_CUSTOM_DOMAINS) })}
                </span>
              )}

              {/* ---------- 2. 平台预置域名（次要：已自动开通，通常无需改动） ---------- */}
              <div className={css.field}>
                <span className={css.stepTitle}>{t("domainSectionDefault")}</span>
                <span className={css.fieldHint}>{t("domainSectionDefaultHint")}</span>
              </div>

              <div className={css.field}>
                <span className={css.fieldLabel}>{t("domainAppId")}</span>
                <input
                  className={css.input}
                  value={labelInput}
                  placeholder={current.slug}
                  disabled={busy}
                  onChange={(event) => {
                    labelTouched.current = true
                    setLabelInput(event.target.value.trim().toLowerCase())
                  }}
                />
                <span className={css.fieldHint}>
                  {t("domainAppIdHint", { slug: current.slug })}
                </span>
                {!labelValid && (
                  <span className={css.fieldErrorInline}>{t("domainAppIdInvalid")}</span>
                )}
              </div>

              <div className={css.field}>
                <span className={css.fieldLabel}>{t("domainSuffixes")}</span>
                <div className={css.chipRow}>
                  {suffixInput.map((suffix) => (
                    <span key={suffix} className={css.targetChip} data-selected="true">
                      {suffix}
                      <button
                        type="button"
                        className={css.chipRemove}
                        disabled={busy}
                        aria-label={`${t("domainRemoveSuffix")} ${suffix}`}
                        onClick={() => {
                          suffixesTouched.current = true
                          setSuffixInput((list) => list.filter((item) => item !== suffix))
                        }}
                      >
                        ×
                      </button>
                    </span>
                  ))}
                </div>
                <SuffixAdder
                  t={t}
                  disabled={busy}
                  onAdd={(suffix) => {
                    suffixesTouched.current = true
                    setSuffixInput((list) => (list.includes(suffix) ? list : [...list, suffix]))
                  }}
                />
                <span className={css.fieldHint}>
                  {t("domainSuffixesHint", { count: String(currentSuffixes.length) })}
                </span>
                {!suffixesValid && (
                  <span className={css.fieldErrorInline}>{t("domainSuffixInvalid")}</span>
                )}
              </div>

              {previewHostnames.length > 0 && (
                <div className={css.field}>
                  <span className={css.fieldLabel}>{t("domainAppIdPreview")}</span>
                  <div className={css.previewTable}>
                    {/* 表头条：环境页签嵌在这里，与下面「当前域名」清单同一套头部样式。 */}
                    <div className={css.domainTableHeadBar}>
                      <span className={css.domainTableHeadTitle}>{t("domainAppIdPreview")}</span>
                      <div className={css.tabBar} role="tablist" aria-label={t("domainTabListLabel")}>
                        <button
                          type="button"
                          role="tab"
                          className={css.tabButton}
                          data-active={environmentTab === ALL_ENVIRONMENTS ? "true" : "false"}
                          aria-selected={environmentTab === ALL_ENVIRONMENTS}
                          onClick={() => { setEnvironmentTab(ALL_ENVIRONMENTS) }}
                        >
                          {t("domainTabAll")}
                          <span className={css.tabCount}>{previewHostnames.length}</span>
                        </button>
                        {previewEnvironments.map((environment) => {
                          const count = filterDomainsByEnvironment(previewHostnames, environment).length
                          return (
                            <button
                              key={environment}
                              type="button"
                              role="tab"
                              className={css.tabButton}
                              data-active={environmentTab === environment ? "true" : "false"}
                              aria-selected={environmentTab === environment}
                              onClick={() => { setEnvironmentTab(environment as EnvironmentTab) }}
                            >
                              {domainStatusLabel("environment", environment, t)}
                              <span className={css.tabCount}>{count}</span>
                            </button>
                          )
                        })}
                      </div>
                    </div>
                    {visiblePreviewHostnames.map((entry) => (
                      <div key={`${entry.environment}-${entry.hostname}`} className={css.previewRow}>
                        <span className={css.envDot} data-environment={entry.environment} />
                        <span className={css.targetChipBadge}>
                          {domainStatusLabel("environment", entry.environment, t)}
                        </span>
                        <code className={css.previewHost}>{entry.hostname}</code>
                        <CopyButton value={entry.hostname} t={t} />
                      </div>
                    ))}
                  </div>
                </div>
              )}

              {/* 保存键不在正文里 —— 它由底部的唯一操作条（footer）承载，
                  正文滚到哪里都留在视口内。这里只留平台域名的只读回显。 */}

              {/* ---------- 当前主机名清单（平台域名的只读回显） ---------- */}
              {defaultDomains.length > 0 && (
                <>
                  <DomainTable
                    domains={defaultDomains}
                    t={t}
                    header={(
                      <span className={css.domainTableHeadTitle}>{t("domainList")}</span>
                    )}
                  />
                </>
              )}
            </>
          )}
        </div>

        {/* 底部是**唯一**的操作条：左边一个状态槽，右边依次是动作与关闭。
            内容与按钮同处一行 ⇒ 底部只有一条横条、一条分隔线。
            状态槽永远单行省略（见 `.footerStatus`），所以它既不会换行，
            也不会把按钮顶到第二行。 */}
        <footer className={css.footer}>
          <span
            className={css.footerStatus}
            data-tone={footerTone}
            role={error !== undefined ? "alert" : notice !== undefined ? "status" : undefined}
            title={error !== undefined || notice !== undefined ? footerText : undefined}
          >
            {footerText}
          </span>
          <button
            type="button"
            className={css.secondaryButton}
            disabled={busy || atCapacity}
            onClick={addDraft}
            title={atCapacity ? t("domainCustomLimitReached", { max: String(MAX_CUSTOM_DOMAINS) }) : undefined}
          >
            {t("domainCustomAdd")}
          </button>
          <button
            type="button"
            className={css.secondaryButton}
            disabled={!canBind}
            onClick={() => { void bindDrafts() }}
          >
            {busy ? t("domainBinding") : t("domainBind")}
          </button>
          <button
            type="button"
            className={css.primaryButton}
            disabled={busy || !dirty || !labelValid || !suffixesValid}
            onClick={() => { void savePlatformDomains() }}
          >
            {busy ? t("domainSaving") : t("domainSave")}
          </button>
          <button type="button" className={css.secondaryButton} disabled={busy} onClick={onClose}>
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

/**
 * 主机名清单：平台组与自定义组共用一张表。
 *
 * 头部是一个**插槽**而不是一个标题字符串：平台组要在同一根头部条里放环境页签，
 * 自定义组只放一个静态标题。这样两组共用同一套表头样式，页签也天然落在
 * 「域名列表 header」里，而不是浮在表格上方。
 */
function DomainTable({
  domains,
  t,
  header,
}: {
  domains: readonly DeployAppDomain[]
  t: PublishingTranslator
  header: ReactNode
}) {
  return (
    <div className={css.domainTable}>
      <div className={css.domainTableHeadBar}>{header}</div>
      {domains.map((domain) => (
        <div key={`${domain.hostname}${domain.pathPrefix ?? ""}`} className={css.domainRow}>
          <div className={css.domainRowMain}>
            <code className={css.domainHost}>{domain.hostname}</code>
            <span className={css.domainRowMeta}>
              <span className={css.targetChipBadge}>
                {domainStatusLabel("environment", domain.environment, t)}
              </span>
              <span className={`${css.statusPill} ${statusClass(domain.bindingStatus)}`}>
                {t("domainBindingStatus")}: {domainStatusLabel("binding", domain.bindingStatus, t)}
              </span>
              <span className={`${css.statusPill} ${verificationClass(domain.verificationStatus)}`}>
                {t("domainVerificationStatus")}: {domainStatusLabel("verification", domain.verificationStatus, t)}
              </span>
              {domain.isCanonical === true && (
                <span className={css.targetChipBadge}>{t("domainCanonical")}</span>
              )}
            </span>
          </div>
          <CopyButton value={domain.hostname} t={t} />
        </div>
      ))}
    </div>
  )
}

/**
 * 一行**已生效**的自定义域名：可看到绑定/校验状态，并可解绑。
 *
 * 解绑按钮直接调 `composition.update` 把该主机名从组合里摘掉 —— 这才是
 * 「删除自定义域名」的实际语义（`deploy_domain` 行是租户资产，不随之删除）。
 */
function CustomDomainRow({
  domain,
  t,
  busy,
  onRemove,
}: {
  domain: DeployAppDomain
  t: PublishingTranslator
  busy: boolean
  onRemove: () => void
}) {
  return (
    <div className={css.domainRow}>
      <div className={css.domainRowMain}>
        <code className={css.domainHost}>{domain.hostname}</code>
        <span className={css.domainRowMeta}>
          <span className={css.targetChipBadge}>
            {domainStatusLabel("environment", domain.environment, t)}
          </span>
          <span className={`${css.statusPill} ${statusClass(domain.bindingStatus)}`}>
            {t("domainBindingStatus")}: {domainStatusLabel("binding", domain.bindingStatus, t)}
          </span>
          <span className={`${css.statusPill} ${verificationClass(domain.verificationStatus)}`}>
            {t("domainVerificationStatus")}: {domainStatusLabel("verification", domain.verificationStatus, t)}
          </span>
        </span>
      </div>
      <div className={css.domainRowActions}>
        <CopyButton value={domain.hostname} t={t} />
        <button
          type="button"
          className={css.dangerGhostButton}
          disabled={busy}
          title={t("domainUnbind", { hostname: domain.hostname })}
          aria-label={t("domainUnbind", { hostname: domain.hostname })}
          onClick={onRemove}
        >
          {t("domainCustomRemove")}
        </button>
      </div>
    </div>
  )
}

/**
 * 一行**待添加**的自定义域名：自由输入 + 实时提示落在哪个区域。
 *
 * 区域提示是「引导」而不是「闸门」：匹配不到时给出去域名管理的引导，但仍然
 * 允许提交 —— 服务端是权威，用户可能拥有本工作区尚未登记的域名。
 */
function CustomDomainDraftRow({
  draft,
  t,
  busy,
  zones,
  onChange,
  onRemove,
}: {
  draft: CustomDraft
  t: PublishingTranslator
  busy: boolean
  zones: readonly DomainZoneResponse[]
  onChange: (value: string) => void
  onRemove: () => void
}) {
  const host = normalizeHostname(draft.hostname)
  const shaped = host === "" || isHostnameShaped(host)
  const matchedZone = host === "" ? undefined : inferDomainZone(host, zones)
  return (
    <div className={css.field}>
      <div className={css.draftRow}>
        <input
          className={css.input}
          value={draft.hostname}
          placeholder={t("domainCustomHostnamePlaceholder")}
          disabled={busy}
          autoFocus
          onChange={(event) => { onChange(event.target.value.trim().toLowerCase()) }}
        />
        <button
          type="button"
          className={css.dangerGhostButton}
          disabled={busy}
          title={t("domainCustomRemove")}
          aria-label={t("domainCustomRemove")}
          onClick={onRemove}
        >
          ×
        </button>
      </div>
      {!shaped && (
        <span className={css.fieldErrorInline}>{t("domainCustomHostnameInvalid")}</span>
      )}
      {shaped && matchedZone !== undefined && (
        <span className={css.zoneHintOk}>
          {t("domainCustomZoneMatched", { zone: matchedZone.apexHostname })}
        </span>
      )}
      {shaped && host !== "" && matchedZone === undefined && (
        <span className={css.zoneHintWarn}>
          {t("domainCustomZoneUnmatched")}
        </span>
      )}
    </div>
  )
}

/** 后缀录入：单个输入 + 回车/按钮，避免一个字段里塞逗号分隔的字符串。 */
function SuffixAdder({
  t,
  disabled,
  onAdd,
}: {
  t: PublishingTranslator
  disabled: boolean
  onAdd: (suffix: string) => void
}) {
  const [draft, setDraft] = useState("")
  const commit = () => {
    const value = draft.trim().toLowerCase().replace(/^\./, "")
    if (value === "" || !isValidSuffix(value)) return
    onAdd(value)
    setDraft("")
  }
  return (
    <div className={css.chipAdder}>
      <input
        className={css.input}
        value={draft}
        placeholder={t("domainSuffixPlaceholder")}
        disabled={disabled}
        onChange={(event) => { setDraft(event.target.value) }}
        onKeyDown={(event) => {
          if (event.key === "Enter") {
            event.preventDefault()
            commit()
          }
        }}
      />
      <button type="button" className={css.secondaryButton} disabled={disabled || draft.trim() === ""} onClick={commit}>
        {t("domainSuffixAdd")}
      </button>
    </div>
  )
}

/** 复制到剪贴板，成功后就地给出反馈（不弹 toast）。 */
function CopyButton({ value, t }: { value: string; t: PublishingTranslator }) {
  const [copied, setCopied] = useState(false)
  if (value === "") return null
  return (
    <button
      type="button"
      className={css.copyButton}
      title={t("domainCopy")}
      onClick={() => {
        void navigator.clipboard?.writeText(value).then(() => {
          setCopied(true)
          window.setTimeout(() => { setCopied(false) }, 1500)
        })
      }}
    >
      {copied ? t("domainCopied") : t("domainCopy")}
    </button>
  )
}

/* ------------------------------------------------------------------ *
 * Helpers
 * ------------------------------------------------------------------ */

/** 一个后缀：小写点分域名，不以点开头/结尾，至少两段。 */
function isValidSuffix(value: string): boolean {
  return /^[a-z0-9]([a-z0-9-]*[a-z0-9])?(\.[a-z0-9]([a-z0-9-]*[a-z0-9])?)+$/.test(value)
}

/**
 * 环境展示名。`AppDomainResponse.environment` 是自由字符串（服务端环境名），
 * 所以先收窄到本地枚举再查表，未知环境回退原文而不是崩掉。
 */



/** 绑定状态 → 配色。绿=已生效，黄=等待，红=失败，灰=其它。 */
function statusClass(status: string): string {
  switch (status.toUpperCase()) {
    case "ACTIVE":
    case "VERIFIED": return css.statusOk
    case "PENDING": return css.statusWarn
    case "FAILED": return css.statusBad
    default: return css.statusNeutral
  }
}

function verificationClass(status: DeployAppDomain["verificationStatus"]): string {
  switch (status) {
    case "VERIFIED":
    case "NOT_REQUIRED": return css.statusOk
    case "PENDING": return css.statusWarn
    case "FAILED":
    case "EXPIRED": return css.statusBad
    default: return css.statusNeutral
  }
}

function messageOf(cause: unknown): string {
  return cause instanceof Error && cause.message ? cause.message : String(cause)
}

/** 409 = 平台唯一性校验失败（appId 或主机名已被占用），给可操作文案。 */
function isConflict(cause: unknown): boolean {
  if (typeof cause !== "object" || cause === null) return false
  const record = cause as { status?: unknown; code?: unknown; message?: unknown }
  if (record.status === 409) return true
  if (typeof record.code === "number") return record.code >= 40900 && record.code < 41000
  if (typeof record.code === "string") return /(?:conflict|already_exists|duplicate|40901)/i.test(record.code)
  return typeof record.message === "string" && /already exists|already registered/i.test(record.message)
}
