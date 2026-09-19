/**
 * Media asset fields (需求 4/5/6): app icon, cover image, and screenshots.
 *
 * **规范随应用类型变化**（不同商店的封面/截图要求并不相同，见
 * `app-store-preview-spec.ts` 的 `mediaSpecForAppKind`）。本组件只做三件事：
 *
 *   1. 按当前 `appKind` 解析出它自己的 icon / cover / 截图档位；
 *   2. 对用户选中的文件做**比例优先**的校验（aspect → 像素下限 → 体积）；
 *   3. 把 `File` 原样交给父对话框 —— 上传发生在应用创建之后（Drive 需要
 *      `appResourceId` 作锚点），所以本组件保持纯展示 + 校验职责。
 *
 * `API_SERVICE` 这类没有界面截图的类型，其 spec 的 `screenshots` 为空数组，
 * 组件会**整块隐去截图区**而不是渲染一个空列表。
 */
import { useEffect, useMemo, useRef, useState, type ChangeEvent } from "react";
import type { PublishingMessageKey, PublishingTranslator } from "../i18n.ts";
import type { DeployAppKind } from "../service/deploy-app-publishing.ts";
import {
  countScreenshots,
  mediaSpecForAppKind,
  validateCover,
  validateIcon,
  validatePreviewSize,
  type PreviewSizeTarget,
} from "../service/app-store-preview-spec.ts";
import css from "./create-deploy-app.module.css";

export interface DeployAppMediaFiles {
  readonly icon?: File
  readonly cover?: File
  readonly screenshots: Record<string, readonly File[]>
}

export interface DeployAppMediaFieldsProps {
  readonly value: DeployAppMediaFiles
  readonly onChange: (next: DeployAppMediaFiles) => void
  readonly t: PublishingTranslator
  /** 当前应用类型：决定用哪一套商店规范（icon/cover/截图档位）。 */
  readonly appKind?: DeployAppKind | undefined
}

interface ProbingImage {
  width: number
  height: number
}

/** Read intrinsic pixel size from a local file via the browser image decoder. */
function probeImage(file: File): Promise<ProbingImage | undefined> {
  return new Promise((resolve) => {
    const url = URL.createObjectURL(file)
    const image = new Image()
    image.onload = () => {
      URL.revokeObjectURL(url)
      resolve({ width: image.naturalWidth, height: image.naturalHeight })
    }
    image.onerror = () => {
      URL.revokeObjectURL(url)
      resolve(undefined)
    }
    image.src = url
  })
}

/** "1320 × 2868" 形式的档位说明，用于档位按钮与错误提示。 */
function sizeLabel(target: PreviewSizeTarget): string {
  return `${target.width} × ${target.height}`
}

/**
 * 设备档位的可读标签。
 *
 * 不用 `device` slug 机械 title-case —— 那会产出 "Wechat Miniprogram"、
 * "Iphone"、"Harmonyos" 这类既不是中文也不是正规英文的字符串。商店里用户
 * 真正看到的设备名是 "iPhone 6.9″"、"Desktop" 这种，因此这里按档位 `key`
 * 出一张人读表，未知 key 才回落到 slug 兜底。
 */
const TARGET_LABEL_KEYS: Readonly<Record<string, PublishingMessageKey>> = {
  "iphone-69": "screenshotTargetIphone69",
  "iphone-67": "screenshotTargetIphone67",
  "iphone-65": "screenshotTargetIphone65",
  "ipad-13": "screenshotTargetIpad13",
  "ipad-129": "screenshotTargetIpad129",
  "android-phone": "screenshotTargetAndroidPhone",
  "android-tablet-10": "screenshotTargetAndroidTablet",
  "harmonyos-phone": "screenshotTargetHarmonyosPhone",
  "mac": "screenshotTargetMac",
  "desktop": "screenshotTargetDesktop",
  "web": "screenshotTargetWeb",
  "browser-extension": "screenshotTargetBrowserExtension",
  "wechat-miniprogram": "screenshotTargetWechatMiniProgram",
  "douyin-miniprogram": "screenshotTargetDouyinMiniProgram",
}

/** 档位标签：优先用本地化设备名，未知档位回落到设备 slug。 */
function targetLabel(target: PreviewSizeTarget, t: PublishingTranslator): string {
  const key = TARGET_LABEL_KEYS[target.key]
  const device = key
    ? t(key)
    : target.device
        .split("-")
        .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
        .join(" ")
  return `${device} ${sizeLabel(target)}`
}

export function DeployAppMediaFields({ value, onChange, t, appKind }: DeployAppMediaFieldsProps) {
  const spec = useMemo(() => mediaSpecForAppKind(appKind), [appKind])
  const targets = spec.screenshots

  const [iconUrl, setIconUrl] = useState<string>()
  const [coverUrl, setCoverUrl] = useState<string>()
  const [error, setError] = useState<string>()
  const [screenshotUrls, setScreenshotUrls] = useState<Record<string, string[]>>({})
  const [selectedTarget, setSelectedTarget] = useState<string>(targets[0]?.key ?? "")
  const iconInputRef = useRef<HTMLInputElement>(null)
  const coverInputRef = useRef<HTMLInputElement>(null)
  const screenshotInputRef = useRef<HTMLInputElement>(null)

  // 应用类型变化 ⇒ 档位集合变化 ⇒ 选中项可能已不存在，回落到第一个档位。
  useEffect(() => {
    if (!targets.some((item) => item.key === selectedTarget)) {
      setSelectedTarget(targets[0]?.key ?? "")
    }
  }, [targets, selectedTarget])

  // Revoke object URLs on unmount.
  useEffect(() => {
    const urls = [iconUrl, coverUrl, ...Object.values(screenshotUrls).flat()].filter(Boolean) as string[]
    return () => { urls.forEach((url) => URL.revokeObjectURL(url)) }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  const accept = spec.acceptedTypes.join(",")
  const target = targets.find((item) => item.key === selectedTarget)
  const targetCount = target ? (value.screenshots[target.key]?.length ?? 0) : 0
  const totalCount = countScreenshots(value.screenshots)
  const typeLabel = spec.acceptedTypes
    .map((type) => type.replace("image/", "").toUpperCase())
    .join(" / ")

  const setIcon = async (file: File) => {
    if (!spec.acceptedTypes.includes(file.type)) {
      setError(t("mediaTypeError", { types: typeLabel }))
      return
    }
    if (file.size > spec.icon.maxBytes) {
      setError(t("mediaBytesError", { name: file.name, max: String(Math.round(spec.icon.maxBytes / 1024 / 1024)) }))
      return
    }
    const probe = await probeImage(file)
    if (probe) {
      const verdict = validateIcon(probe.width, probe.height, spec.icon)
      if (!verdict.ok) {
        setError(t("appIconError", {
          width: String(spec.icon.recommendedEdge),
          height: String(spec.icon.recommendedEdge),
        }))
        return
      }
    }
    setError(undefined)
    setIconUrl((current) => { if (current) URL.revokeObjectURL(current); return URL.createObjectURL(file) })
    onChange({ ...value, icon: file })
  }

  const setCover = async (file: File) => {
    if (!spec.acceptedTypes.includes(file.type)) {
      setError(t("mediaTypeError", { types: typeLabel }))
      return
    }
    if (file.size > spec.cover.maxBytes) {
      setError(t("mediaBytesError", { name: file.name, max: String(Math.round(spec.cover.maxBytes / 1024 / 1024)) }))
      return
    }
    const probe = await probeImage(file)
    if (probe) {
      const verdict = validateCover(probe.width, probe.height, spec.cover)
      if (!verdict.ok) {
        // 比例错与分辨率不足给不同提示 —— 修法完全不同。
        setError(verdict.reason === "aspect"
          ? t("mediaAspectError", { name: file.name, size: `${spec.cover.recommendedWidth} × ${spec.cover.recommendedHeight}` })
          : t("mediaSizeError", {
            name: file.name,
            width: String(spec.cover.minWidth),
            height: String(spec.cover.minHeight),
            tolerance: "min",
          }))
        return
      }
    }
    setError(undefined)
    setCoverUrl((current) => { if (current) URL.revokeObjectURL(current); return URL.createObjectURL(file) })
    onChange({ ...value, cover: file })
  }

  const addScreenshots = async (files: FileList | null) => {
    if (!files || files.length === 0 || !target) return
    const next = [...files]
    for (const file of next) {
      if (!spec.acceptedTypes.includes(file.type)) {
        setError(t("mediaTypeError", { types: typeLabel }))
        return
      }
      const probe = await probeImage(file)
      if (probe) {
        const verdict = validatePreviewSize(probe.width, probe.height, target)
        if (!verdict.ok) {
          setError(t("mediaAspectError", { name: file.name, size: sizeLabel(target) }))
          return
        }
      }
    }
    if (targetCount + next.length > target.max || totalCount + next.length > spec.maxScreenshotsTotal) {
      setError(t("screenshotsLimitError", {
        target: target.max,
        max: String(Math.min(target.max, spec.maxScreenshotsTotal)),
      }))
      return
    }
    setError(undefined)
    const urls = next.map((file) => URL.createObjectURL(file))
    setScreenshotUrls((current) => ({ ...current, [target.key]: [...(current[target.key] ?? []), ...urls] }))
    onChange({
      ...value,
      screenshots: {
        ...value.screenshots,
        [target.key]: [...(value.screenshots[target.key] ?? []), ...next],
      },
    })
  }

  const removeScreenshot = (index: number) => {
    if (!target) return
    const current = value.screenshots[target.key] ?? []
    onChange({
      ...value,
      screenshots: {
        ...value.screenshots,
        [target.key]: current.filter((_, itemIndex) => itemIndex !== index),
      },
    })
    setScreenshotUrls((urls) => ({
      ...urls,
      [target.key]: (urls[target.key] ?? []).filter((_, itemIndex) => itemIndex !== index),
    }))
  }

  const onIconInput = (event: ChangeEvent<HTMLInputElement>) => {
    const file = event.target.files?.[0]
    if (file) void setIcon(file)
    event.target.value = ""
  }

  const onCoverInput = (event: ChangeEvent<HTMLInputElement>) => {
    const file = event.target.files?.[0]
    if (file) void setCover(file)
    event.target.value = ""
  }

  const onScreenshotInput = (event: ChangeEvent<HTMLInputElement>) => {
    void addScreenshots(event.target.files)
    event.target.value = ""
  }

  return (
    <div className={css.mediaGrid}>
      {/* 当前类型的规范说明：让用户在上传前就知道该按哪套商店要求准备。 */}
      <div className={css.mediaSpecNote}>
        <span className={css.mediaSpecNoteLabel}>{t("mediaSpecTitle")}</span>
        <span>{t(spec.noteKey)}</span>
      </div>

      <div className={css.mediaField}>
        <span className={css.fieldLabel}>{t("appIcon")}</span>
        <div className={css.mediaPreview}>
          {iconUrl
            ? <img src={iconUrl} alt="" />
            : <span className={css.mediaPreviewEmpty}>
              {spec.icon.allowAlpha
                ? t("appIconSpecHintAlpha", {
                  edge: String(spec.icon.recommendedEdge),
                  format: typeLabel,
                })
                : t("appIconSpecHint", {
                  edge: String(spec.icon.recommendedEdge),
                  format: typeLabel,
                })}
            </span>}
        </div>
        <div className={css.mediaFileRow}>
          {value.icon && <span className={css.mediaFileName}>{value.icon.name}</span>}
          <button type="button" className={css.secondaryButton} onClick={() => { iconInputRef.current?.click() }}>
            {value.icon ? t("changeImage") : t("chooseImage")}
          </button>
          <input ref={iconInputRef} className={css.fileInputHidden} type="file" accept={accept} onChange={onIconInput} />
        </div>
      </div>

      <div className={css.mediaField}>
        <span className={css.fieldLabel}>{t("coverImage")}</span>
        <div className={css.mediaPreview}>
          {coverUrl
            ? <img src={coverUrl} alt="" />
            : <span className={css.mediaPreviewEmpty}>
              {t("coverImageSpecHint", {
                width: String(spec.cover.recommendedWidth),
                height: String(spec.cover.recommendedHeight),
              })}
            </span>}
        </div>
        <div className={css.mediaFileRow}>
          {value.cover && <span className={css.mediaFileName}>{value.cover.name}</span>}
          <button type="button" className={css.secondaryButton} onClick={() => { coverInputRef.current?.click() }}>
            {value.cover ? t("changeImage") : t("chooseImage")}
          </button>
          <input ref={coverInputRef} className={css.fileInputHidden} type="file" accept={accept} onChange={onCoverInput} />
        </div>
      </div>

      {/* 无截图语义的类型（如 API_SERVICE）整块隐去，不渲染空列表。 */}
      {targets.length > 0 && target && (
        <div className={css.screenshotZone}>
          <span className={css.fieldLabel}>{t("screenshots")}</span>
          <span className={css.fieldHint}>
            {t("screenshotsSpecHint", {
              shelves: String(targets.length),
              total: String(spec.maxScreenshotsTotal),
            })}
          </span>
          <div className={css.screenshotTargets} role="tablist" aria-label={t("screenshotTarget")}>
            {targets.map((item) => (
              <button
                key={item.key}
                type="button"
                role="tab"
                aria-selected={item.key === selectedTarget}
                data-selected={item.key === selectedTarget}
                className={css.targetChip}
                title={targetLabel(item, t)}
                onClick={() => { setSelectedTarget(item.key) }}
              >
                {targetLabel(item, t)}
                {item.min > 0 && (
                  <span className={css.targetChipBadge}>{t("screenshotMinBadge", { min: String(item.min) })}</span>
                )}
              </button>
            ))}
          </div>
          <div className={css.mediaMeta}>
            {t("screenshotLimit", { count: String(targetCount), max: String(target.max) })}
            {target.min > 0 && ` · ${t("screenshotMinNote", { min: String(target.min) })}`}
          </div>
          <div className={css.screenshotGrid}>
            {(value.screenshots[target.key] ?? []).map((file, index) => (
              <div key={`${file.name}-${index}`} className={css.screenshotCard}>
                <img src={screenshotUrls[target.key]?.[index]} alt={file.name} />
                <button type="button" className={css.screenshotRemove} title={t("remove")} aria-label={`${t("remove")} ${file.name}`} onClick={() => { removeScreenshot(index) }}>
                  ×
                </button>
              </div>
            ))}
            {targetCount < target.max && (
              <button type="button" className={css.addScreenshot} onClick={() => { screenshotInputRef.current?.click() }}>
                + {t("addScreenshot")}
              </button>
            )}
            <input ref={screenshotInputRef} className={css.fileInputHidden} type="file" accept={accept} multiple onChange={onScreenshotInput} />
          </div>
        </div>
      )}

      {error && <div className={css.errorBanner} role="alert">{error}</div>}
    </div>
  )
}
