/**
 * AppDetailDrawer — 应用行上的「详情」命令。
 *
 * 一个只读的侧栏，把分散在四个接口里的应用事实收在一屏：身份、发布域名
 * （含 DNS 归属状态与 CNAME 目标）、平台目标、代码来源、生命周期。
 *
 * 三个设计取舍：
 *
 * - **一次并发拉取**。四个读接口互不依赖，`loadAppDetail` 用 `Promise.all`
 *   并发，抽屉一次往返就打开；单个接口失败只影响它那一段（`platformTargets`
 *   与 `sourceRepositories` 各自 catch 成空数组），不会让整屏变错误页 ——
 *   一个还没接过仓库的应用仍然应该能看自己的身份与域名。
 * - **媒体资产从 `metadata.media` 读**，那是创建流程回写的唯一位置
 *   （见 `CreateAppDialog`），没有独立接口。
 * - **不做编辑**。详情是观察面，改配置走各自的命令（域名设置 / 上传代码），
 *   避免同一个字段出现两个写入口。
 */
import { useEffect, useMemo, useState } from "react";
import type { AppResponse, SdkworkDeployAppClient } from "@sdkwork/deployments-app-sdk";
import type { SdkworkDriveAppClient } from "@sdkwork/drive-app-sdk";
import type { DeploymentsLocale } from "@sdkwork/deployments-pc-commons";
import {
  publishingTranslator,
  APP_KIND_LABEL_KEYS,
  APP_STATUS_LABEL_KEYS,
  type PublishingMessageKey,
  type PublishingTranslator,
} from "../i18n.ts";
import {
  createDeployAppOperationsService,
  domainStatusLabel,
  type DeployAppDetail,
  type DeployAppDomain,
  type DeployAppOperationsService,
} from "../service/deploy-app-operations.ts";
import css from "./create-deploy-app.module.css";

export interface AppDetailDrawerProps {
  readonly deployClient: SdkworkDeployAppClient
  readonly driveClient: SdkworkDriveAppClient
  readonly locale: DeploymentsLocale
  /** 列表行已有的数据：先渲染身份，详情到达后补齐其余段落。 */
  readonly app: AppResponse
  readonly onClose: () => void
  readonly theme?: ("light" | "dark") | undefined
  readonly service?: DeployAppOperationsService | undefined
}

export function AppDetailDrawer({
  deployClient,
  driveClient,
  locale,
  app,
  onClose,
  theme = "light",
  service: injectedService,
}: AppDetailDrawerProps) {
  const t = useMemo(() => publishingTranslator(locale), [locale])
  const service = useMemo(
    () => injectedService ?? createDeployAppOperationsService({ deployClient, driveClient }),
    [injectedService, deployClient, driveClient],
  )

  const [detail, setDetail] = useState<DeployAppDetail>()
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string>()

  useEffect(() => {
    let active = true
    setLoading(true)
    void service.loadAppDetail(app.id).then((loaded) => {
      if (active) setDetail(loaded)
    }).catch((cause) => {
      if (active) setError(t("detailLoadFailed", { message: messageOf(cause) }))
    }).finally(() => {
      if (active) setLoading(false)
    })
    return () => { active = false }
  }, [app.id, service, t])

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose()
    }
    document.addEventListener("keydown", onKeyDown)
    const previousOverflow = document.body.style.overflow
    document.body.style.overflow = "hidden"
    return () => {
      document.removeEventListener("keydown", onKeyDown)
      document.body.style.overflow = previousOverflow
    }
  }, [onClose])

  // 列表行已带的字段先渲染，请求回来的同名字段以服务端最新值覆盖。
  const current = detail?.app ?? app
  const domains = detail?.domains ?? []
  const defaultDomains = domains.filter((domain) => domain.kind === "DEFAULT")
  const customDomains = domains.filter((domain) => domain.kind === "CUSTOM")

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
        aria-label={t("detailTitle")}
      >
        <header className={css.header}>
          <div className={css.headerText}>
            <h2>{current.name}</h2>
            <p>{t("detailDescription")}</p>
          </div>
          <button type="button" className={css.closeButton} title={t("close")} aria-label={t("close")} onClick={onClose}>
            ×
          </button>
        </header>

        <div className={css.body}>
          {error && <div className={css.errorBanner} role="alert">{error}</div>}
          {loading && <span className={css.fieldHint}>{t("detailLoading")}</span>}

          {/* ---------- 身份 ---------- */}
          <Section title={t("detailSectionIdentity")}>
            <FieldGrid>
              <Field label={t("detailFieldId")} value={current.id} mono />
              <Field label={t("detailFieldName")} value={current.name} />
              <Field label={t("detailFieldSlug")} value={current.slug} mono />
              <Field label={t("detailFieldKind")} value={enumLabel(current.appKind, APP_KIND_LABEL_KEYS, t)} />
              <Field
                label={t("detailFieldStatus")}
                value={enumLabel(current.appStatus, APP_STATUS_LABEL_KEYS, t)}
              />
              <Field label={t("detailFieldCategory")} value={categoryOf(current, t)} />
              <Field label={t("detailFieldDescription")} value={current.description ?? ""} />
            </FieldGrid>
          </Section>

          {/* ---------- 发布域名 ---------- */}
          <Section title={t("detailSectionDomains")}>
            <FieldGrid>
              <Field label={t("detailFieldAppDomain")} value={current.appDomainLabel ?? current.slug} mono />
              <Field
                label={t("domainSuffixes")}
                value={(current.appDomainSuffixes ?? []).join(" · ")}
              />
            </FieldGrid>
            {domains.length === 0
              ? <span className={css.fieldHint}>{t("domainListEmpty")}</span>
              : (
                <div className={css.domainTable}>
                  <div className={css.domainTableHead}>{t("detailSectionDomains")}</div>
                  {[...defaultDomains, ...customDomains].map((domain) => (
                    <DomainDetailRow key={`${domain.hostname}${domain.pathPrefix ?? ""}`} domain={domain} t={t} />
                  ))}
                </div>
              )}
          </Section>

          {/* ---------- 平台目标 ---------- */}
          <Section title={t("detailSectionTargets")}>
            {detail === undefined
              ? <span className={css.fieldHint}>{t("detailLoading")}</span>
              : detail.platformTargets.length === 0
                ? <span className={css.fieldHint}>{t("detailTargetsEmpty")}</span>
                : (
                  <div className={css.domainTable}>
                    {detail.platformTargets.map((target) => (
                      <div key={target.id} className={css.domainRow}>
                        <div className={css.domainRowMain}>
                          <code className={css.domainHost}>{target.targetKey}</code>
                          <span className={css.domainRowMeta}>
                            <span className={css.targetChipBadge}>{target.platform}</span>
                            <span className={css.targetChipBadge}>{target.techStack}</span>
                            <span className={css.statusPill}>{target.targetStatus}</span>
                            {target.bundleId !== undefined && (
                              <span className={css.fieldHint}>bundle: {target.bundleId}</span>
                            )}
                            {target.packageName !== undefined && (
                              <span className={css.fieldHint}>pkg: {target.packageName}</span>
                            )}
                          </span>
                        </div>
                      </div>
                    ))}
                  </div>
                )}
          </Section>

          {/* ---------- 代码来源 ---------- */}
          <Section title={t("detailSectionSource")}>
            {detail === undefined
              ? <span className={css.fieldHint}>{t("detailLoading")}</span>
              : detail.sourceRepositories.length === 0
                ? <span className={css.fieldHint}>{t("detailSourceEmpty")}</span>
                : detail.sourceRepositories.map((repository) => (
                  <div key={repository.id} className={css.dnsGuide}>
                    <span className={css.dnsGuideTitle}>
                      {t("detailSourceGit")} · {repository.repoKey}
                    </span>
                    <dl className={css.dnsRecord}>
                      <dt>{t("uploadGitProvider")}</dt>
                      <dd><code>{repository.repoProvider}</code></dd>
                      <dt>{t("uploadGitRepoUrl")}</dt>
                      <dd><code>{repository.repoUrl}</code></dd>
                      <dt>{t("uploadGitDefaultBranch")}</dt>
                      <dd><code>{repository.defaultBranch}</code></dd>
                      <dt>{t("uploadGitCloneMode")}</dt>
                      <dd><code>{repository.cloneMode}</code></dd>
                      <dt>Status</dt>
                      <dd><code>{repository.repoStatus}</code></dd>
                    </dl>
                  </div>
                ))}
          </Section>

          {/* ---------- 生命周期 ---------- */}
          <Section title={t("detailSectionLifecycle")}>
            <FieldGrid>
              <Field label={t("detailFieldVersion")} value={current.version} mono />
              <Field label={t("detailFieldDefaultEnvironment")} value={current.defaultEnvironment} />
              <Field
                label={t("detailFieldLatestRelease")}
                value={current.latestReleaseTag ?? t("detailNoValue")}
                mono
              />
              <Field
                label={t("columnPlatformTargets")}
                value={current.platformTargetCount ?? t("detailNoValue")}
              />
              <Field label={t("detailFieldCreated")} value={formatDateTime(current.createdAt, locale)} />
              <Field label={t("detailFieldUpdated")} value={formatDateTime(current.updatedAt, locale)} />
            </FieldGrid>
          </Section>

          {/* ---------- 商店素材（metadata.media） ---------- */}
          {mediaSummary(current) !== undefined && (
            <Section title={t("detailMedia")}>
              <FieldGrid>
                <Field label={t("appIcon")} value={mediaSummary(current)?.icon ?? t("detailNoValue")} />
                <Field label={t("coverImage")} value={mediaSummary(current)?.cover ?? t("detailNoValue")} />
                <Field
                  label={t("createAppMediaTitle")}
                  value={mediaSummary(current)?.screenshots ?? t("detailNoValue")}
                />
              </FieldGrid>
            </Section>
          )}
        </div>

        <footer className={css.footer}>
          <div className={css.footerSpacer} />
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

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section className={css.detailSection}>
      <h3 className={css.detailSectionTitle}>{title}</h3>
      {children}
    </section>
  )
}

function FieldGrid({ children }: { children: React.ReactNode }) {
  return <div className={css.detailGrid}>{children}</div>
}

/** 一个「标签 + 值」格。空值统一渲染为 `—`，不留空行。 */
function Field({ label, value, mono }: { label: string; value: string; mono?: boolean }) {
  return (
    <div className={css.detailField}>
      <dt className={css.detailFieldLabel}>{label}</dt>
      <dd className={mono === true ? css.detailFieldValueMono : css.detailFieldValue}>
        {value === "" ? "—" : value}
      </dd>
    </div>
  )
}

/** 域名明细行：主机名 + 环境/绑定/DNS 状态 + CNAME 目标或待发布 TXT。 */
function DomainDetailRow({ domain, t }: { domain: DeployAppDomain; t: PublishingTranslator }) {
  return (
    <div className={css.domainRow}>
      <div className={css.domainRowMain}>
        <code className={css.domainHost}>{domain.hostname}</code>
        <span className={css.domainRowMeta}>
          <span className={css.targetChipBadge}>{t(domain.kind === "DEFAULT" ? "domainKindDefault" : "domainKindCustom")}</span>
          <span className={css.statusPill}>{domainStatusLabel("environment", domain.environment, t)}</span>
          <span className={css.statusPill}>{domainStatusLabel("binding", domain.bindingStatus, t)}</span>
          <span className={css.statusPill}>{domainStatusLabel("verification", domain.verificationStatus, t)}</span>
          {domain.cnameTarget !== undefined && domain.kind === "CUSTOM" && (
            <span className={css.fieldHint}>
              {t("domainCnameTitle")} → <code>{domain.cnameTarget}</code>
            </span>
          )}
        </span>
        {domain.dnsRecordName !== undefined && (
          <span className={css.fieldHint}>
            {t("domainVerificationPending")}: <code>{domain.dnsRecordName}</code>
            {domain.dnsRecordValue !== undefined && <> = <code>{domain.dnsRecordValue}</code></>}
          </span>
        )}
      </div>
    </div>
  )
}

/* ------------------------------------------------------------------ *
 * Helpers
 * ------------------------------------------------------------------ */

function enumLabel(
  kind: string,
  table: Readonly<Record<string, PublishingMessageKey>>,
  t: PublishingTranslator,
): string {
  const key: PublishingMessageKey | undefined = table[kind]
  return key !== undefined ? t(key) : kind
}

/**
 * `deploy_app.metadata.category` 是创建时写入的分类选择；详情只读展示它。
 * 分类选择形如 `{ primary, secondary?, tertiary? }`，逐段拼成可读路径。
 */
function categoryOf(app: AppResponse, t: PublishingTranslator): string {
  const metadata = app.metadata ?? {}
  const raw = metadata["category"]
  if (typeof raw !== "object" || raw === null) return ""
  const record = raw as Record<string, unknown>
  const segments = [record["primary"], record["secondary"], record["tertiary"]]
    .filter((value): value is string => typeof value === "string" && value !== "")
  if (segments.length === 0) return ""
  // 分类 token 走同一套 message catalog；未登记的 token 回退原文。
  return segments
    .map((segment) => {
      const key = segment as PublishingMessageKey
      const translated = t(key)
      return translated === key ? segment : translated
    })
    .join(" › ")
}

/** 从 `metadata.media` 折出一行可读摘要（数量 + 首个文件标识）。 */
function mediaSummary(app: AppResponse): { icon: string; cover: string; screenshots: string } | undefined {
  const metadata = app.metadata ?? {}
  const media = metadata["media"]
  if (typeof media !== "object" || media === null) return undefined
  const record = media as Record<string, unknown>
  const screenshots = record["screenshots"]
  let screenshotCount = 0
  if (typeof screenshots === "object" && screenshots !== null) {
    for (const list of Object.values(screenshots as Record<string, unknown>)) {
      if (Array.isArray(list)) screenshotCount += list.length
    }
  }
  return {
    icon: describeMediaRef(record["icon"]),
    cover: describeMediaRef(record["cover"]),
    screenshots: screenshotCount === 0 ? "" : String(screenshotCount),
  }
}

function describeMediaRef(value: unknown): string {
  if (typeof value !== "object" || value === null) return ""
  const record = value as Record<string, unknown>
  const name = record["fileName"] ?? record["driveNodeId"] ?? record["nodeId"]
  return typeof name === "string" && name !== "" ? name : ""
}

function formatDateTime(value: string, locale: DeploymentsLocale): string {
  const date = new Date(value)
  return Number.isNaN(date.getTime()) ? value : date.toLocaleString(locale)
}

function messageOf(cause: unknown): string {
  return cause instanceof Error && cause.message ? cause.message : String(cause)
}
