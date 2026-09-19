/**
 * AppDomainDialog — 应用行上的「域名设置」命令（行业标准双轨）。
 *
 * 参考行业 SaaS（Vercel / Netlify / Cloudflare Pages）的通行做法，域名分两类，
 * 本对话框一次覆盖两者：
 *
 *   1. **平台预置域名**（DEFAULT）—— `appId.app.<suffix>`。每个应用都自动获得，
 *      覆盖全部 5 个生命周期环境（`app` / `app-dev` / `app-test` / `app-staging`
 *      / `app-demo`）。服务端在应用创建时已自动开通，这里只允许改 `appId`
 *      （`appDomainLabel`）与后缀目录（`appDomainSuffixes`），保存后服务端重新
 *      对账全部环境。
 *   2. **自定义域名**（CUSTOM）—— 用户自有域名。先向域名区登记主机名并取得
 *      DNS 归属校验记录，再把它绑到应用上；只有绑定后流量才会真正进来。
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
import { useEffect, useMemo, useRef, useState } from "react";
import type { AppResponse, DomainZoneResponse, SdkworkDeployAppClient } from "@sdkwork/deployments-app-sdk";
import type { SdkworkDriveAppClient } from "@sdkwork/drive-app-sdk";
import type { DeploymentsLocale } from "@sdkwork/deployments-pc-commons";
import { publishingTranslator, type PublishingTranslator } from "../i18n.ts";
import {
  createDeployAppOperationsService,
  domainStatusLabel,
  type DeployAppDomain,
  type DeployAppDomainState,
  type DeployAppOperationsService,
} from "../service/deploy-app-operations.ts";
import css from "./create-deploy-app.module.css";

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
  const labelTouched = useRef(false)
  const suffixesTouched = useRef(false)

  // 自定义域名
  const [zones, setZones] = useState<readonly DomainZoneResponse[]>()
  const [zonesLoading, setZonesLoading] = useState(false)
  const [zonesLoaded, setZonesLoaded] = useState(false)
  const [zoneId, setZoneId] = useState("")
  const [customHostname, setCustomHostname] = useState("")

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

  const loadZones = async () => {
    setZonesLoading(true)
    setZonesLoaded(true)
    try {
      const loaded = await service.listDomainZones()
      setZones(loaded)
      if (loaded.length > 0) setZoneId((value) => (value === "" ? loaded[0]?.id ?? "" : value))
    } catch (cause) {
      setZones([])
      setError(t("domainUpdateFailed", { message: messageOf(cause) }))
    } finally {
      setZonesLoading(false)
    }
  }

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

  const bindCustom = async () => {
    setBusy(true)
    setError(undefined)
    setNotice(undefined)
    try {
      const result = await service.bindCustomHostname(app.id, {
        zoneId,
        apexHostname: zones?.find((zone) => zone.id === zoneId)?.apexHostname ?? "",
        hostname: customHostname.trim().toLowerCase(),
      })
      setCustomHostname("")
      await reload()
      const summary = t("domainBound", { hostname: result.hostname.hostname })
      setNotice(summary)
      onChanged?.(summary)
    } catch (cause) {
      setError(isConflict(cause)
        ? t("domainBindConflict")
        : t("domainUpdateFailed", { message: messageOf(cause) }))
    } finally {
      setBusy(false)
    }
  }

  const zoneError = useMemo(() => {
    if (zoneId === "") return undefined
    const apex = zones?.find((zone) => zone.id === zoneId)?.apexHostname
    const host = customHostname.trim().toLowerCase()
    if (apex === undefined || host === "") return undefined
    if (host !== apex && !host.endsWith(`.${apex}`)) {
      return t("domainCustomMustMatchZone", { zone: apex })
    }
    return undefined
  }, [zoneId, zones, customHostname, t])

  const hostnameValid = /^[a-z0-9]([a-z0-9-]*[a-z0-9])?(\.[a-z0-9]([a-z0-9-]*[a-z0-9])?)+$/
    .test(customHostname.trim().toLowerCase())

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
              {/* ---------- 1. 平台预置域名 ---------- */}
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
                  <div className={css.envGrid}>
                    {previewHostnames.map((entry) => (
                      <div key={`${entry.environment}-${entry.hostname}`} className={css.envCard} style={{ cursor: "default" }}>
                        <span className={css.envDot} data-environment={entry.environment} />
                        <span className={css.envName}>{entry.hostname}</span>
                        <span className={css.envId}>{domainStatusLabel("environment", entry.environment, t)}</span>
                      </div>
                    ))}
                  </div>
                </div>
              )}

              <div className={css.modeRow}>
                <button
                  type="button"
                  className={css.primaryButton}
                  disabled={busy || !dirty || !labelValid || !suffixesValid}
                  onClick={() => { void savePlatformDomains() }}
                >
                  {busy ? t("domainSaving") : t("domainSave")}
                </button>
              </div>

              {/* ---------- 当前主机名清单 ---------- */}
              <div className={css.field}>
                <span className={css.stepTitle}>{t("domainList")}</span>
                {defaultDomains.length === 0 && customDomains.length === 0 && (
                  <span className={css.fieldHint}>{t("domainListEmpty")}</span>
                )}
              </div>
              {defaultDomains.length > 0 && (
                <DomainTable domains={defaultDomains} t={t} label={t("domainKindDefault")} />
              )}
              {customDomains.length > 0 && (
                <DomainTable domains={customDomains} t={t} label={t("domainKindCustom")} />
              )}

              {/* ---------- 2. 自定义域名 ---------- */}
              <div className={css.field}>
                <span className={css.stepTitle}>{t("domainSectionCustom")}</span>
                <span className={css.fieldHint}>{t("domainSectionCustomHint")}</span>
              </div>

              <div className={css.field}>
                <span className={css.fieldLabel}>{t("domainCustomZone")}</span>
                {!zonesLoaded && (
                  <button
                    type="button"
                    className={css.secondaryButton}
                    style={{ alignSelf: "flex-start" }}
                    disabled={busy}
                    onClick={() => { void loadZones() }}
                  >
                    {t("domainCustomZoneLoad")}
                  </button>
                )}
                {zonesLoading && <span className={css.fieldHint}>{t("domainListLoading")}</span>}
                {zonesLoaded && zones !== undefined && zones.length === 0 && (
                  <span className={css.fieldHint}>{t("domainCustomZoneEmpty")}</span>
                )}
                {zones !== undefined && zones.length > 0 && (
                  <select
                    className={css.select}
                    value={zoneId}
                    disabled={busy}
                    onChange={(event) => { setZoneId(event.target.value) }}
                  >
                    {zones.map((zone) => (
                      <option key={zone.id} value={zone.id}>
                        {zone.apexHostname} · {zone.verifiedHostnameCount}/{zone.hostnameCount}
                      </option>
                    ))}
                  </select>
                )}
                <span className={css.fieldHint}>{t("domainCustomZoneHint")}</span>
              </div>

              {zones !== undefined && zones.length > 0 && (
                <>
                  <div className={css.field}>
                    <span className={css.fieldLabel}>{t("domainCustomHostname")}</span>
                    <input
                      className={css.input}
                      value={customHostname}
                      placeholder={t("domainCustomHostnamePlaceholder")}
                      disabled={busy}
                      onChange={(event) => { setCustomHostname(event.target.value.trim().toLowerCase()) }}
                    />
                    <span className={css.fieldHint}>{t("domainCustomHostnameHint")}</span>
                    {customHostname !== "" && !hostnameValid && (
                      <span className={css.fieldErrorInline}>{t("domainCustomHostnameInvalid")}</span>
                    )}
                    {zoneError !== undefined && (
                      <span className={css.fieldErrorInline}>{zoneError}</span>
                    )}
                  </div>

                  {/* CNAME 指引 —— 行业标准做法：让用户把自有域名别名到平台主机名。 */}
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

                  <div className={css.modeRow}>
                    <button
                      type="button"
                      className={css.secondaryButton}
                      disabled={busy || zoneId === "" || !hostnameValid || zoneError !== undefined}
                      onClick={() => { void bindCustom() }}
                    >
                      {busy ? t("domainBinding") : t("domainBind")}
                    </button>
                  </div>
                </>
              )}
            </>
          )}
        </div>

        <footer className={css.footer}>
          {error && <div className={css.errorBanner} role="alert">{error}</div>}
          {!error && notice && <div className={css.successBanner} role="status">{notice}</div>}
          {!error && !notice && <div className={css.footerSpacer} />}
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

/** 主机名清单：平台组与自定义组共用一张表，只是标题不同。 */
function DomainTable({
  domains,
  t,
  label,
}: {
  domains: readonly DeployAppDomain[]
  t: PublishingTranslator
  label: string
}) {
  return (
    <div className={css.domainTable}>
      <div className={css.domainTableHead}>{label}</div>
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
