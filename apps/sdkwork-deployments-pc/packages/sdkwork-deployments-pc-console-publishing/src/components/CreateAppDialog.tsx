/**
 * CreateAppDialog — 只创建 `deploy_app` 记录，不做任何发布动作。
 *
 * 为什么单独存在：应用生命周期必须先「有应用」再「发布应用」。原先把创建
 * 与发布压进同一个五步对话框（CreateDeployAppDialog），导致用户必须选好源
 * 码目录才能把应用建出来 —— 本对话框把创建这一步独立出来，只收集应用自身
 * 的资料，落库后立刻可以让用户在应用行上发起发布。
 *
 * 交互（单页，非分步）：
 *   1. 应用类型（appKind，网格；决定平台目标与截图尺寸档位）
 *   2. 应用名称 + slug（留空由名称推导）
 *   3. 应用分类（deploy_app.metadata.category）
 *   4. 应用资料（可选）：图标 / 封面图 / 应用预览效果图（多图，按设备尺寸档位）
 *   5. 应用描述（deploy_app.description）
 *
 * 持久化严格走 sdkwork-deployments 现有表结构：`deploy_app`
 * （name/slug/app_kind/description/metadata），媒体先落 Drive 再以
 * `metadata.media` 回写；创建应用**不**写 `deploy_app_platform_target`，
 * 平台目标由后续发布流程按实际发布的表面写入。
 *
 * 组件为纯 props 输入（生成式 client + locale + 宿主端口），不依赖 console
 * context，deployments 控制台与任何宿主（Web Server 控制台 / BirdCoder 插件）
 * 都能复用（高内聚低耦合）。
 */
import { useCallback, useEffect, useMemo, useState } from "react";
import type { AppKind, AppResponse, SdkworkDeployAppClient } from "@sdkwork/deployments-app-sdk";
import type { SdkworkDriveAppClient } from "@sdkwork/drive-app-sdk";
import type { DeploymentsLocale } from "@sdkwork/deployments-pc-commons";
import { publishingTranslator, type PublishingTranslator } from "../i18n.ts";
import {
  createDeployAppPublishingService,
  appKindOfCard as serviceAppKindOfCard,
  isAppSlugConflictError,
  toSdkAppKind,
  type DeployAppCategorySelection,
  type DeployAppKind,
  type DeployAppMediaGroup,
} from "../service/deploy-app-publishing.ts";
import { CategoryCascadeSelect } from "./CategoryCascadeSelect.tsx";
import { DeployAppTypeGrid } from "./DeployAppTypeGrid.tsx";
import {
  DeployAppMediaFields,
  type DeployAppMediaFiles,
} from "./DeployAppMediaFields.tsx";
import css from "./create-deploy-app.module.css";

export interface CreateAppDialogProps {
  readonly deployClient: SdkworkDeployAppClient
  readonly driveClient: SdkworkDriveAppClient
  readonly locale: DeploymentsLocale
  /** 创建成功回调：宿主据此刷新列表并（可选）提示用户去发布。 */
  readonly onCreated?: ((app: AppResponse, media: DeployAppMediaGroup) => void) | undefined
  readonly onClose: () => void
  /** 主题（驱动组件内建 CSS 变量切换；缺省浅色）。 */
  readonly theme?: ("light" | "dark") | undefined
  /**
   * 呈现形态：`drawer`（缺省，右侧抽屉）或 `modal`（居中模态）。
   * 两者共用同一套 `--pda-*` 令牌与表单实现，只是外壳不同。
   */
  readonly variant?: ("drawer" | "modal") | undefined
  /** 抽屉形态的宽度档位（`lg` 用于字段较多的场景）。 */
  readonly size?: ("md" | "lg") | undefined
}

/**
 * 应用类型选择复用发布流程的同一套卡片网格（`DeployAppTypeGrid`），避免
 * 「创建」与「发布」出现两套应用类型叫法。创建阶段只取卡片的 `appKind`
 * 语义（见 {@link appKindOfCard}），框架 / 源码目录留到发布阶段。
 */
export function CreateAppDialog({
  deployClient,
  driveClient,
  locale,
  onCreated,
  onClose,
  theme = "light",
  variant = "drawer",
  size = "md",
}: CreateAppDialogProps) {
  const t = useMemo(() => publishingTranslator(locale), [locale])
  const service = useMemo(
    () => createDeployAppPublishingService({ deployClient, driveClient }),
    [deployClient, driveClient],
  )

  const [cardId, setCardId] = useState<string>()
  const [name, setName] = useState("")
  const [slug, setSlug] = useState("")
  const [description, setDescription] = useState("")
  const [category, setCategory] = useState<DeployAppCategorySelection>()
  const [media, setMedia] = useState<DeployAppMediaFiles>({ screenshots: {} })
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string>()
  const [uploadingLabel, setUploadingLabel] = useState<string>()

  const appKind = useMemo<DeployAppKind | undefined>(() => {
    if (cardId === undefined) return undefined
    // 卡片 id 即创建阶段的 appKind 选择单位：经契约校验后交给生成式 client。
    return appKindOfCard(cardId)
  }, [cardId])
  // 契约里的 appKind（用于分类级联的类型过滤）—— 仅在已解析出值时可用。
  const sdkAppKind = useMemo<AppKind | undefined>(
    () => (appKind === undefined ? undefined : toSdkAppKind(appKind)),
    [appKind],
  )

  const uploadMedia = useCallback(async (appId: string, files: DeployAppMediaFiles): Promise<DeployAppMediaGroup> => {
    const upload = async (kind: "icon" | "cover" | "screenshot", file: File, targetKey?: string) => {
      setUploadingLabel(t("mediaUploading", { name: file.name }))
      return service.uploadMedia({
        kind,
        file,
        fileName: file.name,
        contentType: file.type || "application/octet-stream",
        targetKey,
      }, appId)
    }

    const group: {
      icon?: DeployAppMediaGroup["icon"]
      cover?: DeployAppMediaGroup["cover"]
      screenshots: DeployAppMediaGroup["screenshots"]
    } = { screenshots: {} }
    if (files.icon) {
      group.icon = await upload("icon", files.icon)
    }
    if (files.cover) {
      group.cover = await upload("cover", files.cover)
    }
    for (const [targetKey, fileList] of Object.entries(files.screenshots)) {
      group.screenshots[targetKey] = []
      for (const file of fileList) {
        const ref = await upload("screenshot", file, targetKey)
        group.screenshots[targetKey] = [...group.screenshots[targetKey], ref]
      }
    }
    return group
  }, [service, t])

  const canSubmit = appKind !== undefined && name.trim() !== ""
  const hasMedia = media.icon !== undefined
    || media.cover !== undefined
    || Object.values(media.screenshots).some((list) => list.length > 0)

  // 抽屉/模态的通用浮层行为：Esc 关闭（提交中不关闭）+ 打开期间锁 body 滚动。
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

  const submit = async () => {
    if (appKind === undefined || name.trim() === "") {
      setError(t("createAppRequiredFields"))
      return
    }
    setBusy(true)
    setError(undefined)
    setUploadingLabel(undefined)
    try {
      // 1) 只登记应用身份（名称/类型/分类/描述）—— 不发布、不写平台目标。
      const created = await service.createAppRecord({
        name,
        appKind,
        ...(slug.trim() === "" ? {} : { slug: slug.trim() }),
        ...(description.trim() === "" ? {} : { description: description.trim() }),
        ...(category === undefined ? {} : { category }),
      })

      // 2) 应用已存在，此时才有 appId 作为 Drive 资源锚点 → 上传资料。
      let mediaGroup: DeployAppMediaGroup = { screenshots: {} }
      if (hasMedia) {
        mediaGroup = await uploadMedia(created.id, media)
        const uploadedCount = (mediaGroup.icon ? 1 : 0)
          + (mediaGroup.cover ? 1 : 0)
          + Object.values(mediaGroup.screenshots).reduce((sum, list) => sum + list.length, 0)
        if (uploadedCount > 0) {
          // 3) 回写 metadata.media，保留创建时写入的 category。
          await deployClient.app.update(created.id, {
            metadata: { ...(created.metadata ?? {}), media: mediaGroup },
          })
        }
      }

      setBusy(false)
      onCreated?.(created, mediaGroup)
    } catch (cause) {
      setError(errorText(cause, t))
      setBusy(false)
    }
  }

  const isDrawer = variant === "drawer"

  return (
    <div
      className={isDrawer ? css.drawerRoot : css.publishDialog}
      data-theme={theme}
      role="presentation"
      onMouseDown={(event) => { if (event.target === event.currentTarget && !busy) onClose() }}
    >
      <div
        className={isDrawer ? `${css.drawerPanel}${size === "lg" ? ` ${css.drawerPanelLg}` : ""}` : css.dialog}
        role="dialog"
        aria-modal="true"
        aria-label={t("createApp")}
      >
        <header className={css.header}>
          <div className={css.headerText}>
            <h2>{t("createAppAction")}</h2>
            <p>{t("createAppDescription")}</p>
          </div>
          <button type="button" className={css.closeButton} title={t("close")} aria-label={t("close")} onClick={onClose}>
            ×
          </button>
        </header>

        <div className={css.body}>
          <DeployAppTypeGrid cardId={cardId} t={t} onChange={setCardId} />

          <div className={css.field}>
            <span className={css.fieldLabel}>{t("applicationName")}</span>
            <input
              className={css.input}
              value={name}
              placeholder={t("applicationNamePlaceholder")}
              onChange={(event) => { setName(event.target.value) }}
            />
            <span className={css.fieldHint}>{t("applicationNameHint")}</span>
          </div>

          <div className={css.field}>
            <span className={css.fieldLabel}>{t("appSlug")}</span>
            <input
              className={css.input}
              value={slug}
              placeholder={t("appSlugPlaceholder")}
              onChange={(event) => { setSlug(event.target.value) }}
            />
            <span className={css.fieldHint}>{t("appSlugHint")}</span>
          </div>

          <div className={css.field}>
            <span className={css.fieldLabel}>{t("category")}</span>
            <span className={css.fieldHint}>{t("categoryHint")}</span>
            <CategoryCascadeSelect appKind={sdkAppKind} value={category} onChange={setCategory} t={t} theme={theme} />
          </div>

          <div className={css.field}>
            <span className={css.stepTitle}>{t("createAppMediaTitle")}</span>
            <span className={css.fieldHint}>{t("createAppMediaHint")}</span>
          </div>
          <DeployAppMediaFields value={media} onChange={setMedia} t={t} appKind={appKind} />

          <div className={css.field}>
            <span className={css.fieldLabel}>{t("description")}</span>
            <textarea
              className={css.textarea}
              value={description}
              placeholder={t("descriptionPlaceholder")}
              onChange={(event) => { setDescription(event.target.value) }}
            />
          </div>
        </div>

        <footer className={css.footer}>
          {error && <div className={css.errorBanner} role="alert">{error}</div>}
          {uploadingLabel && <div className={css.uploadingText}>{uploadingLabel}</div>}
          {!error && <div className={css.footerSpacer} />}
          <button type="button" className={css.secondaryButton} disabled={busy} onClick={onClose}>
            {t("cancel")}
          </button>
          <button type="button" className={css.primaryButton} disabled={busy || !canSubmit} onClick={() => { void submit() }}>
            {busy ? t("creatingApp") : t("createAppAction")}
          </button>
        </footer>
      </div>
    </div>
  )
}

/**
 * 卡片 id → 契约 `AppKind`。
 *
 * 创建阶段一张卡片只落一个 appKind：契约里 Web 表面（H5 / PC 网页 / 静态资源）
 * 都是 `SPA_WEB` / `STATIC_WEB`，桌面端是 `DESKTOP_APP`。映射表本体放在
 * service 层（`CARD_APP_KIND` / {@link appKindOfCard}），因为发布阶段要用它
 * **反查**（`cardsOfAppKind`）—— 正向与反向必须互为逆映射，写在两处必然会漂移。
 */
function appKindOfCard(cardId: string): DeployAppKind | undefined {
  return serviceAppKindOfCard(cardId)
}

function errorText(cause: unknown, t: PublishingTranslator): string {
  // The slug-uniqueness conflict is a routine, recoverable operator error. The
  // server's `detail` is raw rule text, so map the condition to localised,
  // actionable copy instead of echoing it.
  if (isAppSlugConflictError(cause)) {
    return t("appSlugConflict")
  }
  const message = cause instanceof Error && cause.message ? cause.message : String(cause)
  return t("createAppFailed", { message })
}
