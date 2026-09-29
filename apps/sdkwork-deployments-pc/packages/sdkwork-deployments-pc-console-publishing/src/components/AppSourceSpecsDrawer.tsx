/**
 * AppSourceSpecsDrawer — 应用行上的「源码规格」命令。
 *
 * 一屏回答两个问题，面板顺序就是问题顺序：
 *
 *   1. **每个端用哪份源码？** —— 上半「每个端的默认」。这是本能力的核心：
 *      契约把「哪份是默认」存成有序的 `preference` 而不是布尔，所以「默认」
 *      只由「同端里没人排位更低」定义。面板因此显示**完整链条**（默认 + 回退），
 *      而不是只显示一条；并且区分「操作员声明的默认」与「真正生效的那一份」——
 *      后者还要看来源是否已上传（见 `app-source-spec-routing.ts` 的模块注释）。
 *      改默认走一个下拉，落到 `setClientClassDefault`，由纯函数规划两步写入。
 *   2. **这个应用声明了哪些源码？** —— 下半清单，每行可编辑、可「上传到此规格」
 *      （开「接入源码」对话框并锁定本行）、可删除。
 *
 * 三个刻意的取舍：
 *
 * - **规格行是 div 列表，不是 `DataTable`。** 抽屉里的这一列表是「一条记录的字段
 *   摘要」，不是需要排序/选择/分页的台账；宿主已有同款抽屉内列表样式
 *   （`AppDetailDrawer` 的域名段），复用它而不引入第二套表格骨架。
 * - **未上传来源的排位照常显示。** 它不产出路由规则，但正因如此操作员才需要看见它 ——
 *   把「没有来源」的档从链条里藏掉，面板会显示一个实际并不服务的「默认」，那是
 *   比空白更坏的错误。
 * - **表单不做服务端语义校验。** 客户端只校验契约明写的（`CompositionKey` 形状、
 *   同端同排位唯一），`pathPrefix` / `indexFiles` 的语义归投递面 —— 在这里猜会让
 *   表单拒掉服务端本来接受的值。
 */
import { useEffect, useMemo, useState } from "react";
import type {
  AppClientClass,
  AppPublishEnvironment,
  AppResponse,
  AppSourceSpecResponse,
  SdkworkDeployAppClient,
} from "@sdkwork/deployments-pc-console-core/sdk";
import type { SdkworkDriveAppClient } from "@sdkwork/deployments-pc-console-core/sdk";
import type { DeploymentsLocale } from "@sdkwork/deployments-pc-commons";
import {
  publishingTranslator,
  type PublishingMessageKey,
  type PublishingTranslator,
} from "../i18n.ts";
import {
  createDeployAppOperationsService,
  type DeployAppOperationsService,
} from "../service/deploy-app-operations.ts";
import {
  APP_CLIENT_CLASSES,
  APP_PUBLISH_ENVIRONMENTS,
  CLIENT_ARCHITECTURE_LABEL_KEYS,
  CLIENT_ARCHITECTURES,
  CLIENT_CLASS_LABEL_KEYS,
  EMPTY_SOURCE_SPEC_DRAFT,
  ROUTE_UNSERVED,
  RUNTIME_TARGET_LABEL_KEYS,
  RUNTIME_TARGETS,
  SOURCE_BINDING_LABEL_KEYS,
  SOURCE_SPEC_HANDLER_LABEL_KEYS,
  SOURCE_SPEC_STATUS_LABEL_KEYS,
  describeRoutingOverview,
  findRouteConflict,
  offeredRanks,
  preferenceFor,
  resolveAppEnvironment,
  sortRoutes,
  sourceSpecDraftFrom,
  splitIndexFiles,
  validateSourceSpecDraft,
  withRoute,
  type ClientClassRouting,
  type SourceSpecDraft,
  type SourceSpecDraftIssue,
  type SourceSpecRoutingOverview,
} from "../service/app-source-spec-routing.ts";
import { UploadSourceDialog } from "./UploadSourceDialog.tsx";
import css from "./create-deploy-app.module.css";

export interface AppSourceSpecsDrawerProps {
  readonly deployClient: SdkworkDeployAppClient
  readonly driveClient: SdkworkDriveAppClient
  readonly locale: DeploymentsLocale
  /** 目标应用：规格挂在已存在的应用上。 */
  readonly app: AppResponse
  readonly onClose: () => void
  /** 成功回调：宿主据此提示并刷新列表。 */
  readonly onChanged?: ((summary: string) => void) | undefined
  readonly theme?: ("light" | "dark") | undefined
  /** 注入服务实例（测试用）；缺省按 client 构造。 */
  readonly service?: DeployAppOperationsService | undefined
}

/** 处理方式的显示顺序；`WIKI` 放最后因为它不是文件型表面。 */
const HANDLER_CHOICES = ["STATIC", "SPA", "WIKI"] as const

/**
 * 两个枚举下拉的选项表，**顺序取自路由内核的词表**而不是在这里重排。
 *
 * 选项值用内核导出的 `RUNTIME_TARGETS` / `CLIENT_ARCHITECTURES`，文案用
 * `Record<Union, key>` 那张表 ⇒ 契约新增成员时这里自动多一项，而不是悄悄少一项。
 */
const RUNTIME_TARGET_CHOICES = RUNTIME_TARGETS.map(
  (value) => [value, RUNTIME_TARGET_LABEL_KEYS[value]] as const,
)
const CLIENT_ARCHITECTURE_CHOICES = CLIENT_ARCHITECTURES.map(
  (value) => [value, CLIENT_ARCHITECTURE_LABEL_KEYS[value]] as const,
)

export function AppSourceSpecsDrawer({
  deployClient,
  driveClient,
  locale,
  app,
  onClose,
  onChanged,
  theme = "light",
  service: injectedService,
}: AppSourceSpecsDrawerProps) {
  const t = useMemo(() => publishingTranslator(locale), [locale])
  const service = useMemo(
    () => injectedService ?? createDeployAppOperationsService({ deployClient, driveClient }),
    [injectedService, deployClient, driveClient],
  )

  const [environment, setEnvironment] = useState<AppPublishEnvironment>(() => resolveAppEnvironment(app))
  const [specs, setSpecs] = useState<readonly AppSourceSpecResponse[]>()
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string>()
  const [notice, setNotice] = useState<string>()
  const [refresh, setRefresh] = useState(0)

  /** 表单：`create` 或正在编辑哪一条。undefined = 表单关闭。 */
  const [editor, setEditor] = useState<{ mode: "create" } | { mode: "edit"; specId: string }>()
  const [draft, setDraft] = useState<SourceSpecDraft>(EMPTY_SOURCE_SPEC_DRAFT)
  /**
   * 正在给哪一条规格接来源。
   *
   * 承接的界面是 `UploadSourceDialog` 的「绑定源码来源」动作 —— 与「接入源码」
   * 行命令**同一个组件、同一套契约写入**，只是这里把目标规格预设并锁定。行内既然
   * 已经点明了是哪一条，让入口再提供一个能改成别条的下拉只会制造误操作。
   */
  const [attachTarget, setAttachTarget] = useState<AppSourceSpecResponse>()
  const [deleteTarget, setDeleteTarget] = useState<AppSourceSpecResponse>()

  useEffect(() => {
    let active = true
    setBusy(true)
    setError(undefined)
    void service.listSourceSpecs(app.id, environment).then((items) => {
      if (active) setSpecs(items)
    }).catch((cause) => {
      if (active) setError(t("sourceSpecsLoadFailed", { message: messageOf(cause) }))
    }).finally(() => {
      if (active) setBusy(false)
    })
    return () => { active = false }
  }, [app.id, environment, refresh, service, t])

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      // 表单/接入来源/确认层自己处理 Esc；同时监听会让一次 Esc 关掉两层。
      // `attachTarget` 是**完整对话框**，它的 Esc 处理器同样挂在 document 上，
      // 漏掉这一项会让一次按键同时关掉对话框与它背后的抽屉。
      if (event.key === "Escape" && editor === undefined && attachTarget === undefined && deleteTarget === undefined) onClose()
    }
    document.addEventListener("keydown", onKeyDown)
    const previousOverflow = document.body.style.overflow
    document.body.style.overflow = "hidden"
    return () => {
      document.removeEventListener("keydown", onKeyDown)
      document.body.style.overflow = previousOverflow
    }
  }, [attachTarget, deleteTarget, editor, onClose])

  const settled = (summary: string) => {
    setNotice(summary)
    onChanged?.(summary)
    setRefresh((value) => value + 1)
  }

  const rows = specs ?? []
  const overview = useMemo(() => describeRoutingOverview(rows), [rows])
  const editingId = editor?.mode === "edit" ? editor.specId : undefined
  const siblings = useMemo(
    () => rows.filter((spec) => spec.id !== editingId),
    [editingId, rows],
  )

  const openCreate = () => {
    setDraft(EMPTY_SOURCE_SPEC_DRAFT)
    setError(undefined)
    setNotice(undefined)
    setEditor({ mode: "create" })
  }

  const openEdit = (spec: AppSourceSpecResponse) => {
    setDraft(sourceSpecDraftFrom(spec))
    setError(undefined)
    setNotice(undefined)
    setEditor({ mode: "edit", specId: spec.id })
  }

  const saveDraft = async () => {
    const issues = validateSourceSpecDraft(draft, siblings)
    if (issues.length > 0) {
      setError(issues.map((issue) => t(ISSUE_KEYS[issue])).join(" "))
      return
    }
    setBusy(true)
    setError(undefined)
    const routes = draft.routes.map((route) => ({
      clientClass: route.clientClass,
      preference: route.preference,
    }))
    try {
      if (editor?.mode === "edit") {
        const current = rows.find((spec) => spec.id === editor.specId)
        if (current === undefined) {
          // 打开编辑之后这一条从列表里消失了（别处删了 / 切了环境）。此时**不能**
          // 退化成新建：表单里还留着那一份的字段，提交会变成一个凭空多出来的规格。
          // 关掉表单并重载列表，让操作员从真实状态重新开始。
          setEditor(undefined)
          setError(t("sourceSpecsEditTargetGone"))
          setRefresh((value) => value + 1)
        } else {
          // `current.version` 是 `If-Match` 令牌：表单已过期时必须显式失败，
          // 而不是把并发期间的改动覆盖掉。
          await service.updateSourceSpec(app.id, current.id, {
            label: draft.label.trim(),
            runtimeTarget: draft.runtimeTarget,
            clientArchitecture: draft.clientArchitecture,
            clientClassRoutes: routes,
            handler: draft.handler,
            pathPrefix: draft.pathPrefix.trim(),
            indexFiles: [...splitIndexFiles(draft.indexFiles)],
            spaFallback: draft.spaFallback.trim() === "" ? null : draft.spaFallback.trim(),
            isDefault: draft.isDefault,
            status: draft.status,
          }, current.version)
          setEditor(undefined)
          settled(t("sourceSpecsUpdated", { key: current.specKey }))
        }
      } else {
        const created = await service.createSourceSpec(app.id, {
          environment,
          specKey: draft.specKey.trim(),
          label: draft.label.trim(),
          runtimeTarget: draft.runtimeTarget,
          clientArchitecture: draft.clientArchitecture,
          clientClassRoutes: routes,
          handler: draft.handler,
          pathPrefix: draft.pathPrefix.trim(),
          indexFiles: [...splitIndexFiles(draft.indexFiles)],
          ...(draft.spaFallback.trim() === "" ? {} : { spaFallback: draft.spaFallback.trim() }),
          isDefault: draft.isDefault,
        })
        setEditor(undefined)
        settled(t("sourceSpecsCreated", { key: created.specKey }))
      }
    } catch (cause) {
      setError(t("sourceSpecsSaveFailed", { message: messageOf(cause) }))
    } finally {
      setBusy(false)
    }
  }

  const makeDefault = async (clientClass: AppClientClass, targetSpecId: string) => {
    setBusy(true)
    setError(undefined)
    setNotice(undefined)
    const target = rows.find((spec) => spec.id === targetSpecId)
    try {
      await service.setClientClassDefault(app.id, clientClass, targetSpecId, rows)
      settled(t("sourceSpecsRoutingSet", {
        label: target?.label ?? targetSpecId,
        client: t(CLIENT_CLASS_LABEL_KEYS[clientClass]),
      }))
    } catch (cause) {
      setError(t("sourceSpecsRoutingSetFailed", { message: messageOf(cause) }))
    } finally {
      setBusy(false)
    }
  }

  const confirmDelete = async () => {
    if (deleteTarget === undefined) return
    setBusy(true)
    setError(undefined)
    try {
      await service.deleteSourceSpec(app.id, deleteTarget.id)
      setDeleteTarget(undefined)
      settled(t("sourceSpecsDeleted", { key: deleteTarget.specKey }))
    } catch (cause) {
      setError(t("sourceSpecsDeleteFailed", { message: messageOf(cause) }))
    } finally {
      setBusy(false)
    }
  }

  return (
    <div
      className={css.drawerRoot}
      data-theme={theme}
      role="presentation"
      onMouseDown={(event) => { if (event.target === event.currentTarget && !busy) onClose() }}
    >
      <div
        className={`${css.drawerPanel}${` ${css.drawerPanelLg}`}`}
        role="dialog"
        aria-modal="true"
        aria-label={t("sourceSpecsAction")}
      >
        <header className={css.header}>
          <div className={css.headerText}>
            <h2>{t("sourceSpecsTitle", { name: app.name })}</h2>
            <p>{t("sourceSpecsDescription")}</p>
          </div>
          <button type="button" className={css.closeButton} title={t("close")} aria-label={t("close")} onClick={onClose}>
            ×
          </button>
        </header>

        <div className={css.body}>
          {error && <div className={css.errorBanner} role="alert">{error}</div>}
          {!error && notice && <div className={css.successBanner} role="status">{notice}</div>}

          <div className={css.specToolbar}>
            <label className={css.specToolbarField}>
              <span className={css.fieldLabel}>{t("sourceSpecsEnvironment")}</span>
              <select
                className={css.select}
                value={environment}
                disabled={busy}
                aria-label={t("sourceSpecsEnvironment")}
                onChange={(event) => { setEnvironment(event.target.value as AppPublishEnvironment) }}
              >
                {APP_PUBLISH_ENVIRONMENTS.map((value) => (
                  <option key={value} value={value}>{value}</option>
                ))}
              </select>
            </label>
            <button
              type="button"
              className={css.secondaryButton}
              disabled={busy}
              onClick={() => { setNotice(undefined); setRefresh((value) => value + 1) }}
            >
              {t("refresh")}
            </button>
            <button type="button" className={css.primaryButton} disabled={busy} onClick={openCreate}>
              + {t("sourceSpecsAdd")}
            </button>
          </div>

          {specs === undefined
            ? <span className={css.fieldHint}>{t("sourceSpecsLoading")}</span>
            : (
              <>
                <section className={css.detailSection}>
                  <h3 className={css.detailSectionTitle}>{t("sourceSpecsSectionRouting")}</h3>
                  <p className={css.fieldHint}>{t("sourceSpecsRoutingHint")}</p>
                  <div className={css.specRouting}>
                    {overview.classes.map((entry) => (
                      <ClientClassRow
                        key={entry.clientClass}
                        entry={entry}
                        candidates={rows}
                        busy={busy}
                        t={t}
                        onMakeDefault={makeDefault}
                      />
                    ))}
                  </div>
                  <RoutingFootnote overview={overview} t={t} />
                </section>

                <section className={css.detailSection}>
                  <h3 className={css.detailSectionTitle}>
                    {t("sourceSpecsSectionList")}
                    {rows.length > 0 && <span className={css.specCount}>{t("sourceSpecsCount", { count: rows.length })}</span>}
                  </h3>
                  {rows.length === 0
                    ? <span className={css.fieldHint}>{t("sourceSpecsEmpty")}</span>
                    : (
                      <div className={css.specList}>
                        {[...rows]
                          .sort((left, right) => left.specKey.localeCompare(right.specKey))
                          .map((spec) => (
                            <SpecRow
                              key={spec.id}
                              spec={spec}
                              t={t}
                              busy={busy}
                              onEdit={() => { openEdit(spec) }}
                              onAttach={() => { setError(undefined); setNotice(undefined); setAttachTarget(spec) }}
                              onDelete={() => { setError(undefined); setNotice(undefined); setDeleteTarget(spec) }}
                            />
                          ))}
                      </div>
                    )}
                </section>
              </>
            )}
        </div>

        <footer className={css.footer}>
          <div className={css.footerSpacer} />
          <button type="button" className={css.secondaryButton} disabled={busy} onClick={onClose}>
            {t("close")}
          </button>
        </footer>
      </div>

      {editor !== undefined && (
        <SpecEditorPanel
          draft={draft}
          mode={editor.mode}
          siblings={siblings}
          busy={busy}
          t={t}
          onChange={setDraft}
          onCancel={() => { setEditor(undefined); setError(undefined) }}
          onSave={() => { void saveDraft() }}
        />
      )}

      {/*
        给这一条规格接来源 —— 用的是「接入源码」命令那个对话框的**默认动作**，
        预设并锁定本行。不复用一套抽屉内的精简表单：来源有两种（网盘目录 /
        知识库 Wiki）、网盘还分目录与空间根，两套界面必然漂移，而写入的又必须是
        同一份契约。`environment` 原样传回抽屉当时的环境，否则对话框会去读另一个
        环境下的规格集合，这一条在里面根本不存在。
      */}
      {attachTarget !== undefined && (
        <UploadSourceDialog
          app={app}
          deployClient={deployClient}
          driveClient={driveClient}
          environment={environment}
          locale={locale}
          presetSpecId={attachTarget.id}
          service={service}
          theme={theme}
          onClose={() => { setAttachTarget(undefined); setError(undefined) }}
          onUploaded={(summary) => { settled(summary) }}
        />
      )}

      {deleteTarget !== undefined && (
        <ConfirmPanel
          title={t("sourceSpecsDeleteConfirm", { key: deleteTarget.specKey })}
          hint={t("sourceSpecsDeleteConfirmHint")}
          confirmLabel={t("sourceSpecsDelete")}
          busy={busy}
          t={t}
          onConfirm={() => { void confirmDelete() }}
          onCancel={() => { setDeleteTarget(undefined); setError(undefined) }}
        />
      )}
    </div>
  )
}

/* ------------------------------------------------------------------ *
 * 每个端的默认
 * ------------------------------------------------------------------ */

const ISSUE_KEYS: Readonly<Record<SourceSpecDraftIssue, PublishingMessageKey>> = {
  specKeyRequired: "sourceSpecsIssueSpecKeyRequired",
  specKeyInvalid: "sourceSpecsIssueSpecKeyInvalid",
  specKeyTaken: "sourceSpecsIssueSpecKeyTaken",
  labelRequired: "sourceSpecsIssueLabelRequired",
  routeDuplicate: "sourceSpecsIssueRouteDuplicate",
  routeClassNameUnknown: "sourceSpecsIssueRouteClassNameUnknown",
  routeRankNegative: "sourceSpecsIssueRouteRankNegative",
}

function ClientClassRow({
  entry,
  candidates,
  busy,
  t,
  onMakeDefault,
}: {
  entry: ClientClassRouting
  candidates: readonly AppSourceSpecResponse[]
  busy: boolean
  t: PublishingTranslator
  onMakeDefault: (clientClass: AppClientClass, specId: string) => void
}) {
  // 能成为该端默认的候选：本环境全部规格，减去当前已占 rank 0 的那一份
  // —— 它已经是默认，列出来只会让下拉出现一个选不动的选项。
  const holderId = entry.declared?.preference === 0 ? entry.declared.specId : undefined
  const selectable = candidates.filter((spec) => spec.id !== holderId)

  return (
    <div className={css.specRoutingRow}>
      <span className={css.specRoutingClass}>{t(CLIENT_CLASS_LABEL_KEYS[entry.clientClass])}</span>
      <span className={css.specRoutingChain}>
        {entry.chain.length === 0
          ? <span className={css.specRoutingEmpty}>{t("sourceSpecsRoutingUnrouted")}</span>
          : entry.chain.map((link) => (
            <span
              key={`${link.specId}#${link.preference}`}
              className={[
                css.specRoutingChip,
                link.preference === 0 ? css.specRoutingChipDefault : "",
                link.serves ? "" : css.specRoutingChipDark,
              ].filter(Boolean).join(" ")}
              title={t(SOURCE_BINDING_LABEL_KEYS[link.sourceStatus])}
            >
              {link.label}
              <em>
                {link.preference === 0
                  ? t("sourceSpecsRoutingDeclared")
                  : t("sourceSpecsRoutingFallbackRank", { rank: link.preference })}
              </em>
              {!link.serves && <b>{t("sourceSpecsRoutingDark")}</b>}
            </span>
          ))}
      </span>
      <span className={css.specRoutingAction}>
        {selectable.length > 0 && (
          <select
            className={css.select}
            value=""
            disabled={busy}
            aria-label={`${t("sourceSpecsRoutingMakeDefault")} — ${t(CLIENT_CLASS_LABEL_KEYS[entry.clientClass])}`}
            onChange={(event) => {
              if (event.target.value === "") return
              onMakeDefault(entry.clientClass, event.target.value)
            }}
          >
            <option value="">{t("sourceSpecsRoutingMakeDefault")}</option>
            {selectable.map((spec) => (
              <option key={spec.id} value={spec.id}>{spec.label}</option>
            ))}
          </select>
        )}
      </span>
    </div>
  )
}

/**
 * 面板底部的整体判读。
 *
 * 三类状态分开说，因为对操作员是**三种不同的动作**：「默认还没上传来源」
 * 是回退链在正常工作（去看要不要传），「未声明默认」是要补一条，而「完全没来源」
 * 要么传来源、要么接受落到应用级兜底。合成一句「有 N 个端异常」就丢掉了该做什么。
 */
function RoutingFootnote({ overview, t }: { overview: SourceSpecRoutingOverview; t: PublishingTranslator }) {
  const lines: React.ReactNode[] = []

  for (const entry of overview.classes) {
    if (!entry.fallbackApplied || entry.effective === undefined) continue
    lines.push(
      <p key={`fallback-${entry.clientClass}`} className={css.specRoutingNote}>
        {t(CLIENT_CLASS_LABEL_KEYS[entry.clientClass])}:{" "}
        {t("sourceSpecsRoutingFallbackApplied", { label: entry.effective.label })}
      </p>,
    )
  }

  if (overview.missingDeclaredDefault.length > 0) {
    lines.push(
      <p key="missing" className={css.specRoutingNoteWarn}>
        {t("sourceSpecsRoutingNoDefault")}: {overview.missingDeclaredDefault.map((value) => t(CLIENT_CLASS_LABEL_KEYS[value])).join(" · ")}
      </p>,
    )
  }
  if (overview.dark.length > 0) {
    lines.push(
      <p key="dark" className={css.specRoutingNoteWarn}>
        {t("sourceSpecsRoutingDark")}: {overview.dark.map((value) => t(CLIENT_CLASS_LABEL_KEYS[value])).join(" · ")}
      </p>,
    )
  }
  if (lines.length === 0) return null
  return <div className={css.specRoutingNotes}>{lines}</div>
}

/* ------------------------------------------------------------------ *
 * 规格行
 * ------------------------------------------------------------------ */

function SpecRow({
  spec,
  t,
  busy,
  onEdit,
  onAttach,
  onDelete,
}: {
  spec: AppSourceSpecResponse
  t: PublishingTranslator
  busy: boolean
  onEdit: () => void
  /** 给这一条规格接来源 —— 打开「接入源码」对话框的默认动作，并锁定本行。 */
  onAttach: () => void
  onDelete: () => void
}) {
  const routes = sortRoutes(spec.clientClassRoutes)
  return (
    <div className={css.specRow}>
      <div className={css.specRowMain}>
        <span className={css.specRowTitle}>
          <strong>{spec.label}</strong>
          <code className={css.specKey}>{spec.specKey}</code>
          {spec.isDefault && <span className={css.specBadge}>{t("sourceSpecsAppLevelDefault")}</span>}
          <span className={css.specBadge}>{t(SOURCE_SPEC_STATUS_LABEL_KEYS[spec.status])}</span>
        </span>
        <span className={css.specRowMeta}>
          <span className={css.targetChipBadge}>{t(RUNTIME_TARGET_LABEL_KEYS[spec.runtimeTarget])}</span>
          <span className={css.targetChipBadge}>{t(CLIENT_ARCHITECTURE_LABEL_KEYS[spec.clientArchitecture])}</span>
          <span className={css.targetChipBadge}>{t(SOURCE_SPEC_HANDLER_LABEL_KEYS[spec.handler])}</span>
          <span className={css.statusPill}>{t(SOURCE_BINDING_LABEL_KEYS[spec.sourceStatus])}</span>
          {spec.pathPrefix !== "" && <span className={css.fieldHint}>prefix: <code>{spec.pathPrefix}</code></span>}
        </span>
        <span className={css.specRouteChips}>
          {routes.length === 0
            ? <span className={css.fieldHint}>{t("sourceSpecsRoutesNone")}</span>
            : routes.map((route) => (
              <span key={`${route.clientClass}#${route.preference}`} className={css.specRouteChip}>
                {t(CLIENT_CLASS_LABEL_KEYS[route.clientClass])}
                <em>
                  {route.preference === 0
                    ? t("sourceSpecsRoutingDeclared")
                    : t("sourceSpecsRoutingFallbackRank", { rank: route.preference })}
                </em>
              </span>
            ))}
        </span>
        {spec.sourceStatus === "EMPTY" && (
          <span className={css.fieldHint}>{t("sourceSpecsSourceEmpty")}</span>
        )}
      </div>
      <div className={css.specRowActions}>
        <button
          type="button"
          className={spec.sourceStatus === "EMPTY" ? css.primaryButton : css.secondaryButton}
          disabled={busy}
          onClick={onAttach}
        >
          {t("sourceSpecsBindSource")}
        </button>
        <button type="button" className={css.secondaryButton} disabled={busy} onClick={onEdit}>
          {t("editApp")}
        </button>
        <button type="button" className={css.secondaryButton} disabled={busy} onClick={onDelete}>
          {t("sourceSpecsDelete")}
        </button>
      </div>
    </div>
  )
}

/* ------------------------------------------------------------------ *
 * 新增 / 编辑表单
 * ------------------------------------------------------------------ */

function SpecEditorPanel({
  draft,
  mode,
  siblings,
  busy,
  t,
  onChange,
  onSave,
  onCancel,
}: {
  draft: SourceSpecDraft
  mode: "create" | "edit"
  siblings: readonly AppSourceSpecResponse[]
  busy: boolean
  t: PublishingTranslator
  onChange: (next: SourceSpecDraft) => void
  onSave: () => void
  onCancel: () => void
}) {
  const conflict = findRouteConflict(draft.routes)
  // 编辑态 `specKey` 不可改：它是投影出的 variant key，改名会静默把每条
  // 命名它的路由规则指到别处（契约 `UpdateAppSourceSpecRequest` 里根本没有这个字段）。
  const routesForRanking = siblings.map((spec) => ({
    id: spec.id,
    specKey: spec.specKey,
    label: spec.label,
    status: spec.status,
    sourceStatus: spec.sourceStatus,
    clientClassRoutes: spec.clientClassRoutes,
  }))
  const ownRoutes = { clientClassRoutes: draft.routes }

  return (
    <div className={css.specOverlay} role="presentation">
      <div className={css.specPanel} role="dialog" aria-modal="true" aria-label={mode === "create" ? t("sourceSpecsFormCreateTitle") : t("sourceSpecsFormEditTitle")}>
        <header className={css.specPanelHeader}>
          <h3>{mode === "create" ? t("sourceSpecsFormCreateTitle") : t("sourceSpecsFormEditTitle")}</h3>
          <button type="button" className={css.closeButton} title={t("close")} aria-label={t("close")} onClick={onCancel}>×</button>
        </header>
        <div className={css.specPanelBody}>
          <div className={css.specFormRow}>
            <label className={css.field}>
              <span className={css.fieldLabel}>{t("sourceSpecsFieldKey")}</span>
              <input
                className={css.input}
                value={draft.specKey}
                readOnly={mode === "edit"}
                disabled={busy}
                aria-label={t("sourceSpecsFieldKey")}
                onChange={(event) => { onChange({ ...draft, specKey: event.target.value }) }}
              />
              <span className={css.fieldHint}>{t("sourceSpecsFieldKeyHint")}</span>
            </label>
            <label className={css.field}>
              <span className={css.fieldLabel}>{t("sourceSpecsFieldLabel")}</span>
              <input
                className={css.input}
                value={draft.label}
                disabled={busy}
                aria-label={t("sourceSpecsFieldLabel")}
                onChange={(event) => { onChange({ ...draft, label: event.target.value }) }}
              />
            </label>
          </div>

          <div className={css.specFormRow}>
            <label className={css.field}>
              <span className={css.fieldLabel}>{t("sourceSpecsFieldRuntime")}</span>
              <select
                className={css.select}
                value={draft.runtimeTarget}
                disabled={busy}
                aria-label={t("sourceSpecsFieldRuntime")}
                onChange={(event) => { onChange({ ...draft, runtimeTarget: event.target.value as SourceSpecDraft["runtimeTarget"] }) }}
              >
                {RUNTIME_TARGET_CHOICES.map(([value, key]) => (
                  <option key={value} value={value}>{t(key)}</option>
                ))}
              </select>
            </label>
            <label className={css.field}>
              <span className={css.fieldLabel}>{t("sourceSpecsFieldArchitecture")}</span>
              <select
                className={css.select}
                value={draft.clientArchitecture}
                disabled={busy}
                aria-label={t("sourceSpecsFieldArchitecture")}
                onChange={(event) => { onChange({ ...draft, clientArchitecture: event.target.value as SourceSpecDraft["clientArchitecture"] }) }}
              >
                {CLIENT_ARCHITECTURE_CHOICES.map(([value, key]) => (
                  <option key={value} value={value}>{t(key)}</option>
                ))}
              </select>
              <span className={css.fieldHint}>{t("sourceSpecsFieldArchitectureHint")}</span>
            </label>
          </div>

          <div className={css.specFormRow}>
            <label className={css.field}>
              <span className={css.fieldLabel}>{t("sourceSpecsFieldHandler")}</span>
              <select
                className={css.select}
                value={draft.handler}
                disabled={busy}
                aria-label={t("sourceSpecsFieldHandler")}
                onChange={(event) => { onChange({ ...draft, handler: event.target.value as SourceSpecDraft["handler"] }) }}
              >
                {HANDLER_CHOICES.map((value) => (
                  <option key={value} value={value}>{t(SOURCE_SPEC_HANDLER_LABEL_KEYS[value])}</option>
                ))}
              </select>
            </label>
            <label className={css.field}>
              <span className={css.fieldLabel}>{t("sourceSpecsFieldPathPrefix")}</span>
              <input
                className={css.input}
                value={draft.pathPrefix}
                disabled={busy}
                aria-label={t("sourceSpecsFieldPathPrefix")}
                onChange={(event) => { onChange({ ...draft, pathPrefix: event.target.value }) }}
              />
            </label>
          </div>

          <div className={css.specFormRow}>
            <label className={css.field}>
              <span className={css.fieldLabel}>{t("sourceSpecsFieldIndexFiles")}</span>
              <input
                className={css.input}
                value={draft.indexFiles}
                disabled={busy}
                aria-label={t("sourceSpecsFieldIndexFiles")}
                onChange={(event) => { onChange({ ...draft, indexFiles: event.target.value }) }}
              />
              <span className={css.fieldHint}>{t("sourceSpecsFieldIndexFilesHint")}</span>
            </label>
            <label className={css.field}>
              <span className={css.fieldLabel}>{t("sourceSpecsFieldSpaFallback")}</span>
              <input
                className={css.input}
                value={draft.spaFallback}
                disabled={busy}
                aria-label={t("sourceSpecsFieldSpaFallback")}
                onChange={(event) => { onChange({ ...draft, spaFallback: event.target.value }) }}
              />
            </label>
          </div>

          <div className={css.specFormToggles}>
            <label className={css.specToggle} title={t("sourceSpecsAppLevelDefaultHint")}>
              <input
                type="checkbox"
                checked={draft.isDefault}
                disabled={busy}
                onChange={(event) => { onChange({ ...draft, isDefault: event.target.checked }) }}
              />
              <span>{t("sourceSpecsFieldIsDefault")}</span>
              <em>{t("sourceSpecsAppLevelDefaultHint")}</em>
            </label>
            {mode === "edit" && (
              <label className={css.specToggle}>
                <input
                  type="checkbox"
                  checked={draft.status === "ACTIVE"}
                  disabled={busy}
                  onChange={(event) => { onChange({ ...draft, status: event.target.checked ? "ACTIVE" : "DISABLED" }) }}
                />
                <span>{t("sourceSpecsFieldStatus")}</span>
              </label>
            )}
          </div>

          <div className={css.detailSection}>
            <h4 className={css.detailSectionTitle}>{t("sourceSpecsFieldRoutes")}</h4>
            <p className={css.fieldHint}>{t("sourceSpecsFieldRoutesHint")}</p>
            <div className={css.specFormRoutes}>
              {APP_CLIENT_CLASSES.map((clientClass) => {
                const current = preferenceFor(draft.routes, clientClass)
                const ranks = offeredRanks(ownRoutes, clientClass, routesForRanking)
                return (
                  <label key={clientClass} className={css.specFormRouteRow}>
                    <span className={css.specRoutingClass}>{t(CLIENT_CLASS_LABEL_KEYS[clientClass])}</span>
                    <select
                      className={css.select}
                      value={current === undefined ? String(ROUTE_UNSERVED) : String(current)}
                      disabled={busy}
                      aria-label={`${t("sourceSpecsFieldRoutes")} — ${t(CLIENT_CLASS_LABEL_KEYS[clientClass])}`}
                      onChange={(event) => {
                        const rank = Number(event.target.value)
                        onChange({ ...draft, routes: withRoute(draft.routes, clientClass, rank) })
                      }}
                    >
                      <option value={String(ROUTE_UNSERVED)}>{t("sourceSpecsRouteOption")}</option>
                      {ranks.map((rank) => (
                        <option key={rank} value={String(rank)}>
                          {rank === 0 ? t("sourceSpecsRouteDefaultOption") : t("sourceSpecsRouteRankOption", { rank })}
                        </option>
                      ))}
                    </select>
                  </label>
                )
              })}
            </div>
            {conflict !== undefined && (
              <p className={css.specRoutingNoteWarn}>
                {t("sourceSpecsIssueRouteDuplicate")} ({t(CLIENT_CLASS_LABEL_KEYS[conflict.clientClass])} · {conflict.preference})
              </p>
            )}
          </div>
        </div>
        <footer className={css.specPanelFooter}>
          <button type="button" className={css.secondaryButton} disabled={busy} onClick={onCancel}>
            {t("cancel")}
          </button>
          <button type="button" className={css.primaryButton} disabled={busy} onClick={onSave}>
            {busy ? t("saving") : t("save")}
          </button>
        </footer>
      </div>
    </div>
  )
}

/* ------------------------------------------------------------------ *
 * 确认层
 * ------------------------------------------------------------------ */

function ConfirmPanel({
  title,
  hint,
  confirmLabel,
  busy,
  t,
  onConfirm,
  onCancel,
}: {
  title: string
  hint: string
  confirmLabel: string
  busy: boolean
  t: PublishingTranslator
  onConfirm: () => void
  onCancel: () => void
}) {
  return (
    <div className={css.specOverlay} role="presentation">
      <div className={css.specPanel} role="alertdialog" aria-modal="true" aria-label={title}>
        <header className={css.specPanelHeader}>
          <h3>{title}</h3>
        </header>
        <div className={css.specPanelBody}>
          <p className={css.fieldHint}>{hint}</p>
        </div>
        <footer className={css.specPanelFooter}>
          <button type="button" className={css.secondaryButton} disabled={busy} onClick={onCancel}>
            {t("cancel")}
          </button>
          <button type="button" className={css.primaryButton} disabled={busy} onClick={onConfirm}>
            {confirmLabel}
          </button>
        </footer>
      </div>
    </div>
  )
}

/* ------------------------------------------------------------------ *
 * Helpers
 * ------------------------------------------------------------------ */

function messageOf(cause: unknown): string {
  return cause instanceof Error && cause.message ? cause.message : String(cause)
}
