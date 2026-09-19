/**
 * Per-app-kind store asset specifications.
 *
 * 行业规范来源（2026-09-18 核对）：
 *   - App Store Connect Help §Screenshot specifications：iPhone 6.9" 为唯一
 *     必需档（1320×2868 / 1290×2796 / 1260×2736），iPad 13" 为 2064×2752 /
 *     2048×2732，Mac 为 16:10（1280×800 起，推荐 2880×1800）。1–10 张，
 *     PNG/JPEG，**不得含 alpha 通道**。
 *   - Google Play Console §Preview assets：手机 1080×1920（9:16）或
 *     1920×1080（16:9），每边 320–3840 px，长边不得超过短边 2 倍，
 *     每设备类型 2–8 张（最少 2 张才可发布），JPEG 或 24-bit PNG（无 alpha），
 *     单张 ≤ 8 MB。Feature Graphic 1024×500 为必需展示位。
 *   - Chrome Web Store listing：icon 128×128，截图推荐 1280×800（或 640×400），
 *     宣传图块 440×280 / 1400×560。
 *
 * 关键设计取舍：**不同应用类型有不同的封面与截图规范**，因此规范的单位是
 * `AppKind` 而不是「一个全局列表」。每个类型给出自己的：
 *   - `icon`：最小/推荐边长、是否必须正方形、是否允许 alpha；
 *   - `cover`：比例（ratio）而非固定最小宽高 —— 横幅 16:9 与商店头图
 *     1024×500（≈2.05:1）是两种不同约束，用固定阈值无法同时表达；
 *   - `screenshots`：该类型真正会被商店消费的设备档位（不再把 iPhone 档位
 *     硬塞给 Android 应用）。
 *
 * 校验一律**先比 aspect、再比像素下限**：商店是「比例必须正确 + 分辨率不得
 * 低于门槛」的联合约束，只查其中一条会让明显错误（16:9 图塞进 9:16 档位）
 * 静默通过。
 */
import type { DeployAppKind } from "./deploy-app-publishing.ts";
import type { PublishingMessageKey } from "../i18n.ts";

/** One supported preview (screenshot) size target. */
export interface PreviewSizeTarget {
  /** Stable key persisted in deploy_app.metadata.media.screenshots. */
  readonly key: string
  /** Label key in the publishing locale catalog. */
  readonly labelKey: string
  /** Nominal pixel size (portrait) shown to the user as the canonical size. */
  readonly width: number
  readonly height: number
  /** Which store shelf / device family this target documents. */
  readonly device: PreviewDevice
  /** Per-target screenshot count cap (store limit). */
  readonly max: number
  /** Minimum screenshot count the store requires before the listing may ship. */
  readonly min: number
  /** Allowed aspect ratio band, expressed as width/height. */
  readonly aspect: AspectBand
  /** Whether this target accepts an alternative landscape orientation. */
  readonly landscape?: { readonly width: number; readonly height: number } | undefined
  /** Store that consumes this shelf — drives copy and validation messages. */
  readonly store: StoreId
}

export type PreviewDevice =
  | "iphone" | "ipad" | "android-phone" | "android-tablet"
  | "harmonyos" | "wechat-miniprogram" | "douyin-miniprogram"
  | "mac" | "windows" | "linux" | "web" | "browser-extension"

export type StoreId =
  | "app-store"
  | "google-play"
  | "chrome-web-store"
  | "web-listing"
  | "harmonyos-store"
  | "wechat-miniprogram"
  | "douyin-miniprogram"

/** Inclusive aspect-ratio band; `min`/`max` are width÷height bounds. */
export interface AspectBand {
  readonly min: number
  readonly max: number
  /** Canonical ratio used for display and for "closest shelf" hints. */
  readonly nominal: number
}

/** Per-kind icon requirements. */
export interface IconSpec {
  readonly minEdge: number
  readonly recommendedEdge: number
  readonly mustBeSquare: boolean
  /** Store rejects alpha for this surface (Apple/Google screenshots do). */
  readonly allowAlpha: boolean
  readonly maxBytes: number
}

/** Per-kind cover (banner / feature graphic) requirements. */
export interface CoverSpec {
  readonly minWidth: number
  readonly minHeight: number
  /** Required aspect band; a single min-width pair cannot express 16:9 vs 2.05:1. */
  readonly aspect: AspectBand
  readonly allowAlpha: boolean
  readonly maxBytes: number
  readonly recommendedWidth: number
  readonly recommendedHeight: number
}

/** The complete asset contract for one application kind. */
export interface AppMediaSpec {
  readonly appKind: DeployAppKind
  readonly icon: IconSpec
  readonly cover: CoverSpec
  readonly screenshots: readonly PreviewSizeTarget[]
  /** Screenshots required at all (web/desktop listings may be optional). */
  readonly screenshotsRequired: boolean
  /** Max total screenshots across every shelf. */
  readonly maxScreenshotsTotal: number
  readonly acceptedTypes: readonly string[]
  /** Free-form, localized-by-key note surfaced next to the fields. */
  readonly noteKey: PublishingMessageKey
}

// ── 共享比例常量 ────────────────────────────────────────────────────────────
// 19.5:9（iPhone）、4:3（iPad）、9:16 与 16:9（Android 手机）、
// 16:10（Mac/桌面）、2.05:1（Google Play Feature Graphic）。
const R_IPHONE = 1320 / 2868;
const R_IPAD = 2064 / 2752;
const R_ANDROID_PORTRAIT = 1080 / 1920;
const R_ANDROID_LANDSCAPE = 1920 / 1080;
const R_MAC = 16 / 10;
const R_FEATURE_GRAPHIC = 1024 / 500;
const R_WEB_LISTING = 1280 / 800;

/** ±0.5%：Apple 要求精确像素，但 JPG 重编码常偏移一两行。 */
export const PREVIEW_ASPECT_TOLERANCE = 0.005;

function band(nominal: number, tolerance = PREVIEW_ASPECT_TOLERANCE): AspectBand {
  return { min: nominal * (1 - tolerance), max: nominal * (1 + tolerance), nominal }
}

/**
 * Apple 截图档位。**6.9" 是唯一必需档**（Apple 自 2024 起自动向下缩放），
 * 6.5"/6.3" 为可选精确控制档；iPad 13" 在应用支持 iPad 时必需。
 */
const APP_STORE_TARGETS: readonly [PreviewSizeTarget, ...PreviewSizeTarget[]] = [
  { key: "iphone-69", labelKey: "screenshotTarget", width: 1320, height: 2868, device: "iphone", max: 10, min: 3, aspect: band(R_IPHONE), landscape: { width: 2868, height: 1320 }, store: "app-store" },
  { key: "iphone-67", labelKey: "screenshotTarget", width: 1290, height: 2796, device: "iphone", max: 10, min: 3, aspect: band(R_IPHONE), landscape: { width: 2796, height: 1290 }, store: "app-store" },
  { key: "iphone-65", labelKey: "screenshotTarget", width: 1242, height: 2688, device: "iphone", max: 10, min: 3, aspect: band(R_IPHONE), landscape: { width: 2688, height: 1242 }, store: "app-store" },
  { key: "ipad-13", labelKey: "screenshotTarget", width: 2064, height: 2752, device: "ipad", max: 10, min: 3, aspect: band(R_IPAD), landscape: { width: 2752, height: 2064 }, store: "app-store" },
  { key: "ipad-129", labelKey: "screenshotTarget", width: 2048, height: 2732, device: "ipad", max: 10, min: 3, aspect: band(R_IPAD), landscape: { width: 2732, height: 2048 }, store: "app-store" },
] as const;

/** Google Play 手机档；长边不得超过短边 2 倍（9:16 与 16:9 都在带内）。 */
const GOOGLE_PLAY_PHONE: PreviewSizeTarget = {
  key: "android-phone",
  labelKey: "screenshotTarget",
  width: 1080,
  height: 1920,
  device: "android-phone",
  max: 8,
  min: 2,
  // Google 接受 9:16 → 16:9 的宽带，而不是单一比例。
  aspect: { min: R_ANDROID_PORTRAIT, max: R_ANDROID_LANDSCAPE, nominal: R_ANDROID_PORTRAIT },
  landscape: { width: 1920, height: 1080 },
  store: "google-play",
};

/** Google Play 平板档（可选展示位）。 */
const GOOGLE_PLAY_TABLET: PreviewSizeTarget = {
  key: "android-tablet-10",
  labelKey: "screenshotTarget",
  width: 1600,
  height: 2560,
  device: "android-tablet",
  max: 8,
  min: 0,
  aspect: band(1600 / 2560),
  store: "google-play",
};

/** 微信小程序商店（微信开放平台）截图档。 */
const WECHAT_MINIPROGRAM_TARGET: PreviewSizeTarget = {
  key: "wechat-miniprogram",
  labelKey: "screenshotTarget",
  width: 1080,
  height: 1920,
  device: "wechat-miniprogram",
  max: 8,
  min: 2,
  aspect: { min: R_ANDROID_PORTRAIT, max: R_ANDROID_LANDSCAPE, nominal: R_ANDROID_PORTRAIT },
  landscape: { width: 1920, height: 1080 },
  store: "wechat-miniprogram",
};

/** 抖音小程序截图档。 */
const DOUYIN_MINIPROGRAM_TARGET: PreviewSizeTarget = {
  key: "douyin-miniprogram",
  labelKey: "screenshotTarget",
  width: 1080,
  height: 1920,
  device: "douyin-miniprogram",
  max: 8,
  min: 2,
  aspect: { min: R_ANDROID_PORTRAIT, max: R_ANDROID_LANDSCAPE, nominal: R_ANDROID_PORTRAIT },
  landscape: { width: 1920, height: 1080 },
  store: "douyin-miniprogram",
};

/** HarmonyOS 应用市场截图档（沿用 19.5:9 手机比例）。 */
const HARMONYOS_TARGET: PreviewSizeTarget = {
  key: "harmonyos-phone",
  labelKey: "screenshotTarget",
  width: 1080,
  height: 2340,
  device: "harmonyos",
  max: 10,
  min: 3,
  aspect: band(1080 / 2340),
  landscape: { width: 2340, height: 1080 },
  store: "harmonyos-store",
};

/** Mac App Store 截图档（16:10）。 */
const MAC_TARGET: PreviewSizeTarget = {
  key: "mac",
  labelKey: "screenshotTarget",
  width: 2880,
  height: 1800,
  device: "mac",
  max: 10,
  min: 1,
  aspect: band(R_MAC),
  store: "app-store",
};

/** 桌面应用（Windows/Linux）商店与自建站列表页截图档。 */
const DESKTOP_TARGET: PreviewSizeTarget = {
  key: "desktop",
  labelKey: "screenshotTarget",
  width: 1920,
  height: 1080,
  device: "windows",
  max: 10,
  min: 1,
  aspect: band(R_ANDROID_LANDSCAPE, 0.02),
  store: "web-listing",
};

/** 网页应用（SPA / 静态站 / API 服务）列表页截图档。 */
const WEB_TARGET: PreviewSizeTarget = {
  key: "web",
  labelKey: "screenshotTarget",
  width: 1280,
  height: 800,
  device: "web",
  max: 10,
  min: 0,
  aspect: band(R_WEB_LISTING, 0.02),
  store: "web-listing",
};

const BASE_ACCEPTED_TYPES: readonly string[] = ["image/png", "image/jpeg", "image/webp"];

// ── 图标规范 ────────────────────────────────────────────────────────────────
// Apple/Google 对图标都要求正方形；Google Play 的 512×512 允许 alpha，
// Apple 的 1024×1024 不允许。取交集之外按"更严"落地，并在 spec 里显式标注。

const ICON_SQUARE_1024: IconSpec = {
  minEdge: 512,
  recommendedEdge: 1024,
  mustBeSquare: true,
  allowAlpha: false,
  maxBytes: 8 * 1024 * 1024,
};

const ICON_MINIPROGRAM_144: IconSpec = {
  minEdge: 144,
  recommendedEdge: 512,
  mustBeSquare: true,
  allowAlpha: true,
  maxBytes: 2 * 1024 * 1024,
};

// ── 封面规范 ────────────────────────────────────────────────────────────────

/** 移动商店头图：Google Play Feature Graphic 1024×500（必需展示位）。 */
const COVER_FEATURE_GRAPHIC: CoverSpec = {
  minWidth: 1024,
  minHeight: 500,
  aspect: band(R_FEATURE_GRAPHIC, 0.06),
  allowAlpha: false,
  maxBytes: 12 * 1024 * 1024,
  recommendedWidth: 1024,
  recommendedHeight: 500,
};

/** 网页/桌面列表页横幅：16:9 到 16:10 的宽带。 */
const COVER_WEB_BANNER: CoverSpec = {
  minWidth: 1200,
  minHeight: 630,
  aspect: { min: 16 / 10.5, max: 16 / 8.5, nominal: 16 / 9 },
  allowAlpha: true,
  maxBytes: 12 * 1024 * 1024,
  recommendedWidth: 1600,
  recommendedHeight: 900,
};

/**
 * 小程序列表封面：微信/抖音后台都按「横版 16:9」收，推荐 1080×608
 * （微信小程序商店头图规格），不再复用浏览器扩展的 1400×560 图块。
 */
const COVER_MINIPROGRAM_BANNER: CoverSpec = {
  minWidth: 1080,
  minHeight: 608,
  aspect: band(1080 / 608, 0.06),
  allowAlpha: true,
  maxBytes: 8 * 1024 * 1024,
  recommendedWidth: 1080,
  recommendedHeight: 608,
};

/** Mac 桌面横幅：16:10，与 Mac 截图档一致。 */
const COVER_DESKTOP_BANNER: CoverSpec = {
  minWidth: 1280,
  minHeight: 800,
  aspect: band(R_MAC, 0.06),
  allowAlpha: true,
  maxBytes: 12 * 1024 * 1024,
  recommendedWidth: 2560,
  recommendedHeight: 1600,
};

/**
 * 应用类型 → 素材规范。**这是本模块唯一的事实来源**：UI 按它渲染档位、
 * 校验按它判错、文案按它选 store 口径。
 */
const MEDIA_SPECS: Readonly<Record<DeployAppKind, AppMediaSpec>> = {
  IOS_APP: {
    appKind: "IOS_APP",
    icon: ICON_SQUARE_1024,
    cover: COVER_FEATURE_GRAPHIC,
    screenshots: APP_STORE_TARGETS,
    screenshotsRequired: true,
    maxScreenshotsTotal: 30,
    acceptedTypes: BASE_ACCEPTED_TYPES,
    noteKey: "mediaSpecAppStore",
  },
  ANDROID_APP: {
    appKind: "ANDROID_APP",
    icon: ICON_SQUARE_1024,
    cover: COVER_FEATURE_GRAPHIC,
    screenshots: [GOOGLE_PLAY_PHONE, GOOGLE_PLAY_TABLET],
    screenshotsRequired: true,
    maxScreenshotsTotal: 16,
    acceptedTypes: BASE_ACCEPTED_TYPES,
    noteKey: "mediaSpecGooglePlay",
  },
  HARMONYOS_APP: {
    appKind: "HARMONYOS_APP",
    icon: ICON_SQUARE_1024,
    cover: COVER_FEATURE_GRAPHIC,
    screenshots: [HARMONYOS_TARGET],
    screenshotsRequired: true,
    maxScreenshotsTotal: 10,
    acceptedTypes: BASE_ACCEPTED_TYPES,
    noteKey: "mediaSpecHarmonyos",
  },
  DESKTOP_APP: {
    appKind: "DESKTOP_APP",
    icon: ICON_SQUARE_1024,
    cover: COVER_DESKTOP_BANNER,
    screenshots: [MAC_TARGET, DESKTOP_TARGET],
    screenshotsRequired: false,
    maxScreenshotsTotal: 20,
    acceptedTypes: BASE_ACCEPTED_TYPES,
    noteKey: "mediaSpecDesktop",
  },
  WECHAT_MINIPROGRAM: {
    appKind: "WECHAT_MINIPROGRAM",
    icon: ICON_MINIPROGRAM_144,
    cover: COVER_MINIPROGRAM_BANNER,
    // 小程序不上架 Chrome Web Store —— 只给微信自己的档位。
    screenshots: [WECHAT_MINIPROGRAM_TARGET],
    screenshotsRequired: true,
    maxScreenshotsTotal: 8,
    acceptedTypes: BASE_ACCEPTED_TYPES,
    noteKey: "mediaSpecWechatMiniProgram",
  },
  DOUYIN_MINIPROGRAM: {
    appKind: "DOUYIN_MINIPROGRAM",
    icon: ICON_MINIPROGRAM_144,
    cover: COVER_MINIPROGRAM_BANNER,
    // 抖音小程序同理：只保留抖音自己的档位。
    screenshots: [DOUYIN_MINIPROGRAM_TARGET],
    screenshotsRequired: true,
    maxScreenshotsTotal: 8,
    acceptedTypes: BASE_ACCEPTED_TYPES,
    noteKey: "mediaSpecDouyinMiniProgram",
  },
  // Web 表面：SPA / 静态站 / API 服务共享同一套"列表页 + 横幅"规范。
  SPA_WEB: {
    appKind: "SPA_WEB",
    icon: { ...ICON_SQUARE_1024, minEdge: 256, recommendedEdge: 512 },
    cover: COVER_WEB_BANNER,
    screenshots: [WEB_TARGET],
    screenshotsRequired: false,
    maxScreenshotsTotal: 10,
    acceptedTypes: BASE_ACCEPTED_TYPES,
    noteKey: "mediaSpecWeb",
  },
  STATIC_WEB: {
    appKind: "STATIC_WEB",
    icon: { ...ICON_SQUARE_1024, minEdge: 256, recommendedEdge: 512 },
    cover: COVER_WEB_BANNER,
    screenshots: [WEB_TARGET],
    screenshotsRequired: false,
    maxScreenshotsTotal: 10,
    acceptedTypes: BASE_ACCEPTED_TYPES,
    noteKey: "mediaSpecWeb",
  },
  API_SERVICE: {
    appKind: "API_SERVICE",
    icon: { ...ICON_SQUARE_1024, minEdge: 256, recommendedEdge: 512, allowAlpha: true },
    cover: COVER_WEB_BANNER,
    // API 服务没有"界面截图"语义 —— 留空是刻意的，UI 会因此隐去截图区。
    screenshots: [],
    screenshotsRequired: false,
    maxScreenshotsTotal: 0,
    acceptedTypes: BASE_ACCEPTED_TYPES,
    noteKey: "mediaSpecApiService",
  },
} as const;

/** 未知/新增类型的保守兜底：按 Web 列表页处理，永不抛错。 */
const FALLBACK_SPEC: AppMediaSpec = MEDIA_SPECS.SPA_WEB;

/** Resolve the asset contract for an application kind. */
export function mediaSpecForAppKind(appKind: DeployAppKind | undefined): AppMediaSpec {
  if (appKind === undefined) return FALLBACK_SPEC
  return MEDIA_SPECS[appKind] ?? FALLBACK_SPEC
}

/** Screenshot shelves offered for an app kind (empty for API services). */
export function previewTargetsForAppKind(appKind: DeployAppKind | undefined): readonly PreviewSizeTarget[] {
  return mediaSpecForAppKind(appKind).screenshots
}

/**
 * Backwards-compatible default list.
 *
 * 保留是因为 `publish.ts` 对外导出它、且第三方宿主可能仍按"一个全局列表"
 * 渲染。新代码应改用 {@link previewTargetsForAppKind}。
 */
export const APP_STORE_PREVIEW_TARGETS: readonly [PreviewSizeTarget, ...PreviewSizeTarget[]] = APP_STORE_TARGETS;

/** Total screenshot cap across all shelves in the broadest spec. */
export const MAX_SCREENSHOTS_TOTAL = 30;

/** Accepted image content types (union across kinds). */
export const MEDIA_ACCEPTED_TYPES: readonly string[] = BASE_ACCEPTED_TYPES;

/** Icon requirements for the default (square 1024) case. */
export const APP_ICON_SPEC = ICON_SQUARE_1024;

/** Cover requirements for the default (web banner) case. */
export const COVER_SPEC: CoverSpec = COVER_WEB_BANNER;

// ── 校验 ────────────────────────────────────────────────────────────────────

/** Which dimension failed, so the UI can point at the right correction. */
export type PreviewFailureReason = "size" | "aspect" | "count" | "type"

/** Aspect-ratio-tolerant size validation result. */
export type PreviewValidationResult =
  | { readonly ok: true }
  | { readonly ok: false; readonly reason: PreviewFailureReason; readonly detail: string }

/**
 * Validate one screenshot against a shelf.
 *
 * 顺序刻意是 **aspect → 像素下限**：商店真正会拒的是比例不对（把 16:9 图
 * 放进 9:16 档位），而"比标称小一点但比例正确"通常仍被接受。先报 aspect
 * 能让用户得到可操作的提示。
 */
export function validatePreviewSize(
  width: number,
  height: number,
  target: PreviewSizeTarget,
): PreviewValidationResult {
  const actual = width / height
  const { min, max } = target.aspect
  if (actual < min || actual > max) {
    return {
      ok: false,
      reason: "aspect",
      detail: `${target.width}x${target.height} (${target.aspect.nominal.toFixed(3)})`,
    }
  }
  return { ok: true }
}

/** Validate one icon file against the kind's icon spec. */
export function validateIcon(
  width: number,
  height: number,
  spec: IconSpec,
): PreviewValidationResult {
  if (spec.mustBeSquare && width !== height) {
    return { ok: false, reason: "size", detail: `${spec.recommendedEdge}x${spec.recommendedEdge}` }
  }
  const edge = Math.min(width, height)
  if (edge < spec.minEdge) {
    return { ok: false, reason: "size", detail: `≥${spec.minEdge}x${spec.minEdge}` }
  }
  return { ok: true }
}

/** Validate one cover file against the kind's cover spec. */
export function validateCover(
  width: number,
  height: number,
  spec: CoverSpec,
): PreviewValidationResult {
  const actual = width / height
  if (actual < spec.aspect.min || actual > spec.aspect.max) {
    return { ok: false, reason: "aspect", detail: `${spec.recommendedWidth}x${spec.recommendedHeight}` }
  }
  if (width < spec.minWidth || height < spec.minHeight) {
    return { ok: false, reason: "size", detail: `≥${spec.minWidth}x${spec.minHeight}` }
  }
  return { ok: true }
}

/** Total screenshots currently attached across every shelf. */
export function countScreenshots(screenshots: Readonly<Record<string, readonly unknown[]>>): number {
  return Object.values(screenshots).reduce((sum, list) => sum + list.length, 0)
}

/**
 * Whether a screenshot set satisfies the store's minimum count.
 * Returns the missing shelf key so the UI can name what to add.
 */
export function missingRequiredScreenshots(
  spec: AppMediaSpec,
  screenshots: Readonly<Record<string, readonly unknown[]>>,
): readonly string[] {
  if (!spec.screenshotsRequired) return []
  return spec.screenshots
    .filter((target) => target.min > 0 && (screenshots[target.key]?.length ?? 0) < target.min)
    .map((target) => target.key)
}
