/**
 * Deploy-app publishing service.
 *
 * One high-cohesion facade over the deploy app-api (`/app/v3/api/apps`) and the
 * Drive uploader. It maps the create-deploy-app dialog model onto the existing
 * deploy schema:
 *
 * - `deploy_app`                      <- apps.create (name / slug / app_kind / description / metadata)
 * - `deploy_app_platform_target`      <- apps.platformTargets.create
 * - `deploy_app.metadata` (JSONB)     <- category / media / version / releaseNotes
 * - Drive node refs (icon/cover/screenshots) <- drive uploader, referenced from metadata.media
 *
 * The service is UI-framework-free: callers (deployments console or the
 * birdcoder publish plugin) construct it with the two generated clients and a
 * Drive upload result type, so it stays reusable and decoupled.
 */
import type { SdkworkDeployAppClient } from "@sdkwork/deployments-app-sdk";
import type {
  AppKind,
  AppResponse,
  AppStatus,
  CreatePlatformTargetRequest,
  CreateAppRequest,
  PageInfo,
  Platform,
  TechStack,
} from "@sdkwork/deployments-app-sdk";
import type {
  DriveUploaderBlobLike,
  DriveUploaderUploadResult,
  SdkworkDriveAppClient,
} from "@sdkwork/drive-app-sdk";
import { uuid } from "@sdkwork/utils/id";
import {
  APP_SURFACE_DIRECTORY_SUFFIX,
  type AppSurfaceId,
  type DeployDeploymentMode,
  type DeployEnvironmentId,
  type DeployProjectProfile,
} from "./project-detection.ts";

/**
 * Contract parity note: the Rust authority (`crates/sdkwork-deploy-contract`
 * `src/app_delivery.rs`) already defines `DESKTOP_APP` plus the
 * `WINDOWS`/`MACOS`/`LINUX` platforms; the generated TypeScript SDK union
 * lags behind. The dialog speaks the verbatim contract strings and widens the
 * generated unions locally until the SDK regen lands.
 */
export type DeployAppKind = AppKind | "DESKTOP_APP";
export type DeployPlatform = Platform | "WINDOWS" | "MACOS" | "LINUX";

/**
 * Runtime mirrors of the generated SDK unions above. They exist so the wire
 * boundary can be *checked* instead of cast: `value as AppKind` would happily
 * forward a typo to the backend, while these lists fail fast on anything the
 * contract does not define. Regenerating the SDK with new members means adding
 * them here too — `satisfies` keeps the two in step.
 */
const SDK_APP_KINDS = [
  "STATIC_WEB",
  "SPA_WEB",
  "API_SERVICE",
  "WECHAT_MINIPROGRAM",
  "DOUYIN_MINIPROGRAM",
  "IOS_APP",
  "ANDROID_APP",
  "HARMONYOS_APP",
] as const satisfies readonly AppKind[];
const SDK_PLATFORMS = ["WEB", "API", "WECHAT", "DOUYIN", "IOS", "ANDROID", "HARMONYOS"] as const satisfies readonly Platform[];
/** Contract members the Rust authority accepts but the generated union lacks. */
const CONTRACT_APP_KIND_EXTENSIONS = ["DESKTOP_APP"] as const satisfies readonly DeployAppKind[];
const CONTRACT_PLATFORM_EXTENSIONS = ["WINDOWS", "MACOS", "LINUX"] as const satisfies readonly DeployPlatform[];

function isSdkAppKind(value: DeployAppKind): value is AppKind {
  return (SDK_APP_KINDS as readonly DeployAppKind[]).includes(value);
}

function isSdkPlatform(value: DeployPlatform): value is Platform {
  return (SDK_PLATFORMS as readonly DeployPlatform[]).includes(value);
}

/**
 * Bridge the dialog's locally widened union onto the generated SDK union.
 *
 * Known SDK members pass through unchanged; the documented contract extensions
 * pass through verbatim so the backend remains the single source of truth;
 * anything else is a programming error and throws rather than reaching the wire.
 */
export function toSdkAppKind(value: DeployAppKind): AppKind {
  if (isSdkAppKind(value)) return value;
  if (CONTRACT_APP_KIND_EXTENSIONS.includes(value)) return value as AppKind;
  throw new Error(`unsupported appKind "${value}"`);
}

export function toSdkPlatform(value: DeployPlatform): Platform {
  if (isSdkPlatform(value)) return value;
  if (CONTRACT_PLATFORM_EXTENSIONS.includes(value)) return value as Platform;
  throw new Error(`unsupported platform "${value}"`);
}

/** One selectable application type in the dialog (resolution row). */
export interface DeployAppTypeOption {
  readonly id: string
  readonly appKind: DeployAppKind
  readonly platform: DeployPlatform
  readonly techStack?: TechStack | undefined
  /** sdkwork-specs `apps/` surface this option publishes (root when absent). */
  readonly surface?: AppSurfaceId | undefined
  /** Locale key for the option label. */
  readonly labelKey: import("../i18n.ts").PublishingMessageKey
  /** Locale key for the helper text. */
  readonly hintKey: import("../i18n.ts").PublishingMessageKey
}

/**
 * Dialog-level application types (需求: h5/pc 网页/PC 桌面/小程序/Android/iOS/
 * 鸿蒙/API/静态资源). Surfaces follow sdkwork-specs `APPLICATION_SPEC.md`:
 * `apps/sdkwork-<application-code>-<suffix>`; unsurfaced kinds publish the
 * repository root.
 *
 * Rows are the *resolution targets* of the framework registry below — one row
 * per meaningful (appKind, platform, techStack) combination so the platform
 * target's `targetKey` stays human-readable.
 */
export const DEPLOY_APP_TYPE_OPTIONS: readonly DeployAppTypeOption[] = [
  // Web surfaces.
  { id: "h5", appKind: "SPA_WEB", platform: "WEB", surface: "h5", labelKey: "typeH5", hintKey: "appTypeHint" },
  { id: "uniapp-h5", appKind: "SPA_WEB", platform: "WEB", techStack: "UNI_APP", surface: "h5", labelKey: "typeH5", hintKey: "appTypeHint" },
  { id: "pc-web", appKind: "SPA_WEB", platform: "WEB", surface: "pc", labelKey: "typePcWeb", hintKey: "appTypeHint" },
  // PC desktop (contract extension; see DeployAppKind above).
  { id: "desktop", appKind: "DESKTOP_APP", platform: "WINDOWS", surface: "desktop", labelKey: "typeDesktop", hintKey: "appTypeHint" },
  { id: "desktop-electron", appKind: "DESKTOP_APP", platform: "WINDOWS", surface: "desktop", labelKey: "typeDesktop", hintKey: "appTypeHint" },
  { id: "desktop-tauri", appKind: "DESKTOP_APP", platform: "WINDOWS", surface: "desktop", labelKey: "typeDesktop", hintKey: "appTypeHint" },
  { id: "desktop-qt", appKind: "DESKTOP_APP", platform: "WINDOWS", surface: "desktop", labelKey: "typeDesktop", hintKey: "appTypeHint" },
  { id: "desktop-flutter", appKind: "DESKTOP_APP", platform: "WINDOWS", techStack: "FLUTTER", surface: "desktop", labelKey: "typeDesktop", hintKey: "appTypeHint" },
  // Mini programs.
  { id: "wechat-mini-program", appKind: "WECHAT_MINIPROGRAM", platform: "WECHAT", techStack: "NATIVE", surface: "mini-program", labelKey: "typeWechatMiniProgram", hintKey: "appTypeHint" },
  { id: "douyin-mini-program", appKind: "DOUYIN_MINIPROGRAM", platform: "DOUYIN", techStack: "NATIVE", surface: "mini-program", labelKey: "typeDouyinMiniProgram", hintKey: "appTypeHint" },
  { id: "taro-wechat-mini-program", appKind: "WECHAT_MINIPROGRAM", platform: "WECHAT", techStack: "OTHER", surface: "mini-program", labelKey: "typeWechatMiniProgram", hintKey: "appTypeHint" },
  { id: "uniapp-wechat-mini-program", appKind: "WECHAT_MINIPROGRAM", platform: "WECHAT", techStack: "UNI_APP", surface: "mini-program", labelKey: "typeWechatMiniProgram", hintKey: "appTypeHint" },
  // Android.
  { id: "native-android", appKind: "ANDROID_APP", platform: "ANDROID", techStack: "NATIVE", surface: "android", labelKey: "typeNativeAndroid", hintKey: "appTypeHint" },
  { id: "flutter-android", appKind: "ANDROID_APP", platform: "ANDROID", techStack: "FLUTTER", surface: "android", labelKey: "typeFlutterAndroid", hintKey: "appTypeHint" },
  { id: "react-native-android", appKind: "ANDROID_APP", platform: "ANDROID", techStack: "OTHER", surface: "android", labelKey: "typeNativeAndroid", hintKey: "appTypeHint" },
  { id: "uniapp-android", appKind: "ANDROID_APP", platform: "ANDROID", techStack: "UNI_APP", surface: "android", labelKey: "typeNativeAndroid", hintKey: "appTypeHint" },
  // iOS.
  { id: "native-ios", appKind: "IOS_APP", platform: "IOS", techStack: "NATIVE", surface: "ios", labelKey: "typeNativeIos", hintKey: "appTypeHint" },
  { id: "flutter-ios", appKind: "IOS_APP", platform: "IOS", techStack: "FLUTTER", surface: "ios", labelKey: "typeFlutterIos", hintKey: "appTypeHint" },
  { id: "react-native-ios", appKind: "IOS_APP", platform: "IOS", techStack: "OTHER", surface: "ios", labelKey: "typeNativeIos", hintKey: "appTypeHint" },
  { id: "uniapp-ios", appKind: "IOS_APP", platform: "IOS", techStack: "UNI_APP", surface: "ios", labelKey: "typeNativeIos", hintKey: "appTypeHint" },
  // HarmonyOS.
  { id: "harmonyos", appKind: "HARMONYOS_APP", platform: "HARMONYOS", techStack: "NATIVE", surface: "harmony", labelKey: "typeHarmonyos", hintKey: "appTypeHint" },
  { id: "uniapp-harmonyos", appKind: "HARMONYOS_APP", platform: "HARMONYOS", techStack: "UNI_APP", surface: "harmony", labelKey: "typeHarmonyos", hintKey: "appTypeHint" },
  // Backend / static.
  { id: "api-service", appKind: "API_SERVICE", platform: "API", labelKey: "typeApiService", hintKey: "appTypeHint" },
  { id: "api-service-rust", appKind: "API_SERVICE", platform: "API", techStack: "RUST", labelKey: "typeApiService", hintKey: "appTypeHint" },
  { id: "api-service-node", appKind: "API_SERVICE", platform: "API", techStack: "NODE", labelKey: "typeApiService", hintKey: "appTypeHint" },
  { id: "api-service-go", appKind: "API_SERVICE", platform: "API", techStack: "GO", labelKey: "typeApiService", hintKey: "appTypeHint" },
  { id: "api-service-java", appKind: "API_SERVICE", platform: "API", techStack: "JAVA", labelKey: "typeApiService", hintKey: "appTypeHint" },
  { id: "api-service-python", appKind: "API_SERVICE", platform: "API", techStack: "OTHER", labelKey: "typeApiService", hintKey: "appTypeHint" },
  { id: "static-web", appKind: "STATIC_WEB", platform: "WEB", labelKey: "typeStaticWeb", hintKey: "appTypeHint" },
] as const;

/** Grid icon vocabulary rendered by DeployAppTypeGrid. */
export type DeployAppTypeIconId =
  | "h5"
  | "pc"
  | "desktop"
  | "mini-program"
  | "android"
  | "ios"
  | "harmony"
  | "api"
  | "static";

/**
 * One framework / architecture choice for a primary type card (v3).
 *
 * 行业参照：Vercel/Railway 的导入流程都按目录信号推断框架，并据此给出
 * 构建产物目录默认值（`buildOutputPath`，相对应用根）。`techStack` 仅为
 * 契约 (`TechStack`) 有对应成员时填写；其余框架信息写入 metadata.framework。
 */
export interface DeployFrameworkOption {
  readonly id: string
  /** Resolution row id in {@link DEPLOY_APP_TYPE_OPTIONS}. */
  readonly optionId: string
  readonly labelKey: import("../i18n.ts").PublishingMessageKey
  /** Suggested build-output directory, relative to the application root. */
  readonly buildOutputPath?: string | undefined
  /**
   * v3.2: directory names that identify this framework when ALL of them
   * appear as child directories of the application root (`unpackage` →
   * uni-app, `.dart_tool` → Flutter, `src-tauri` → Tauri, `.nuxt` → Nuxt, …).
   * Only decisive, framework-specific names are listed; generic names like
   * `src`/`dist` are deliberately excluded.
   */
  readonly detectDirectories?: readonly string[] | undefined
}

/**
 * Primary type card shown in the first-step grid (icon + name). Framework /
 * architecture selection lives inside the directory step (v3.2), below the
 * path fields, and is auto-detected from the chosen directory.
 */
export interface DeployAppTypeCard {
  readonly id: string
  readonly iconKey: DeployAppTypeIconId
  readonly labelKey: import("../i18n.ts").PublishingMessageKey
  readonly hintKey: import("../i18n.ts").PublishingMessageKey
  /** Framework / architecture choices for the directory step. */
  readonly frameworks: readonly DeployFrameworkOption[]
  readonly defaultFrameworkId: string
  readonly surface?: AppSurfaceId | undefined
}

/** First-step grid cards, in display order. */
export const DEPLOY_APP_TYPE_CARDS: readonly DeployAppTypeCard[] = [
  {
    id: "h5",
    iconKey: "h5",
    labelKey: "typeH5",
    hintKey: "typeH5Hint",
    surface: "h5",
    defaultFrameworkId: "react",
    frameworks: [
      { id: "react", optionId: "h5", labelKey: "fwReact", buildOutputPath: "dist" },
      { id: "vue", optionId: "h5", labelKey: "fwVue", buildOutputPath: "dist" },
      { id: "next", optionId: "h5", labelKey: "fwNext", buildOutputPath: "out", detectDirectories: [".next"] },
      { id: "nuxt", optionId: "h5", labelKey: "fwNuxt", buildOutputPath: ".output/public", detectDirectories: [".nuxt"] },
      { id: "uniapp", optionId: "uniapp-h5", labelKey: "fwUniapp", buildOutputPath: "unpackage/dist/build/h5", detectDirectories: ["unpackage"] },
      { id: "capacitor", optionId: "h5", labelKey: "fwCapacitor", buildOutputPath: "dist", detectDirectories: ["ios", "android"] },
    ],
  },
  {
    id: "pc-web",
    iconKey: "pc",
    labelKey: "typePcWeb",
    hintKey: "typePcWebHint",
    surface: "pc",
    defaultFrameworkId: "react",
    frameworks: [
      { id: "react", optionId: "pc-web", labelKey: "fwReact", buildOutputPath: "dist" },
      { id: "vue", optionId: "pc-web", labelKey: "fwVue", buildOutputPath: "dist" },
      { id: "next", optionId: "pc-web", labelKey: "fwNext", buildOutputPath: "out", detectDirectories: [".next"] },
      { id: "nuxt", optionId: "pc-web", labelKey: "fwNuxt", buildOutputPath: ".output/public", detectDirectories: [".nuxt"] },
      { id: "svelte", optionId: "pc-web", labelKey: "fwSvelte", buildOutputPath: "dist" },
    ],
  },
  {
    id: "desktop",
    iconKey: "desktop",
    labelKey: "typeDesktop",
    hintKey: "typeDesktopHint",
    surface: "desktop",
    defaultFrameworkId: "electron",
    frameworks: [
      { id: "electron", optionId: "desktop-electron", labelKey: "fwElectron", buildOutputPath: "dist" },
      { id: "tauri", optionId: "desktop-tauri", labelKey: "fwTauri", buildOutputPath: "dist", detectDirectories: ["src-tauri"] },
      { id: "qt", optionId: "desktop-qt", labelKey: "fwQt", buildOutputPath: "build" },
      { id: "flutter", optionId: "desktop-flutter", labelKey: "fwFlutterDesktop", buildOutputPath: "build", detectDirectories: [".dart_tool"] },
    ],
  },
  {
    id: "mini-program",
    iconKey: "mini-program",
    labelKey: "typeMiniProgram",
    hintKey: "typeMiniProgramHint",
    surface: "mini-program",
    defaultFrameworkId: "wechat-native",
    frameworks: [
      { id: "wechat-native", optionId: "wechat-mini-program", labelKey: "fwWechatNative", buildOutputPath: "dist" },
      { id: "douyin-native", optionId: "douyin-mini-program", labelKey: "fwDouyinNative", buildOutputPath: "dist" },
      { id: "taro", optionId: "taro-wechat-mini-program", labelKey: "fwTaro", buildOutputPath: "dist" },
      { id: "uniapp", optionId: "uniapp-wechat-mini-program", labelKey: "fwUniapp", buildOutputPath: "unpackage/dist/build/mp-weixin", detectDirectories: ["unpackage"] },
    ],
  },
  {
    id: "android",
    iconKey: "android",
    labelKey: "typeAndroid",
    hintKey: "typeAndroidHint",
    surface: "android",
    defaultFrameworkId: "kotlin",
    frameworks: [
      { id: "kotlin", optionId: "native-android", labelKey: "fwKotlin", buildOutputPath: "app/build/outputs/apk" },
      { id: "java", optionId: "native-android", labelKey: "fwJava", buildOutputPath: "app/build/outputs/apk" },
      { id: "flutter", optionId: "flutter-android", labelKey: "fwFlutter", buildOutputPath: "build/app/outputs/flutter-apk", detectDirectories: [".dart_tool"] },
      { id: "react-native", optionId: "react-native-android", labelKey: "fwReactNative", buildOutputPath: "android/app/build/outputs/apk", detectDirectories: ["android"] },
      { id: "uniapp", optionId: "uniapp-android", labelKey: "fwUniapp", buildOutputPath: "unpackage/dist/build/app", detectDirectories: ["unpackage"] },
    ],
  },
  {
    id: "ios",
    iconKey: "ios",
    labelKey: "typeIos",
    hintKey: "typeIosHint",
    surface: "ios",
    defaultFrameworkId: "swift",
    frameworks: [
      { id: "swift", optionId: "native-ios", labelKey: "fwSwift", buildOutputPath: "build/Build/Products" },
      { id: "objc", optionId: "native-ios", labelKey: "fwObjc", buildOutputPath: "build/Build/Products" },
      { id: "flutter", optionId: "flutter-ios", labelKey: "fwFlutter", buildOutputPath: "build/ios/iphoneos", detectDirectories: [".dart_tool"] },
      { id: "react-native", optionId: "react-native-ios", labelKey: "fwReactNative", buildOutputPath: "ios/build/Build/Products", detectDirectories: ["ios"] },
      { id: "uniapp", optionId: "uniapp-ios", labelKey: "fwUniapp", buildOutputPath: "unpackage/dist/build/ios", detectDirectories: ["unpackage"] },
    ],
  },
  {
    id: "harmonyos",
    iconKey: "harmony",
    labelKey: "typeHarmonyos",
    hintKey: "typeHarmonyosHint",
    surface: "harmony",
    defaultFrameworkId: "arkts",
    frameworks: [
      { id: "arkts", optionId: "harmonyos", labelKey: "fwArkTS", buildOutputPath: "build" },
      { id: "uniapp", optionId: "uniapp-harmonyos", labelKey: "fwUniapp", buildOutputPath: "unpackage/dist/build/app", detectDirectories: ["unpackage"] },
    ],
  },
  {
    id: "api-service",
    iconKey: "api",
    labelKey: "typeApiService",
    hintKey: "typeApiServiceHint",
    defaultFrameworkId: "rust",
    frameworks: [
      { id: "rust", optionId: "api-service-rust", labelKey: "fwRust", buildOutputPath: "target/release", detectDirectories: ["target"] },
      { id: "node", optionId: "api-service-node", labelKey: "fwNode", buildOutputPath: "dist" },
      { id: "go", optionId: "api-service-go", labelKey: "fwGo", buildOutputPath: "bin", detectDirectories: ["cmd"] },
      { id: "java", optionId: "api-service-java", labelKey: "fwSpring", buildOutputPath: "build/libs" },
      { id: "python", optionId: "api-service-python", labelKey: "fwPython" },
    ],
  },
  {
    id: "static-web",
    iconKey: "static",
    labelKey: "typeStaticWeb",
    hintKey: "typeStaticWebHint",
    defaultFrameworkId: "plain",
    frameworks: [
      { id: "plain", optionId: "static-web", labelKey: "fwPlain", buildOutputPath: "." },
      { id: "hugo", optionId: "static-web", labelKey: "fwHugo", buildOutputPath: "public", detectDirectories: ["content"] },
      { id: "hexo", optionId: "static-web", labelKey: "fwHexo", buildOutputPath: "public", detectDirectories: ["themes"] },
      { id: "vitepress", optionId: "static-web", labelKey: "fwVitepress", buildOutputPath: ".vitepress/dist", detectDirectories: [".vitepress"] },
    ],
  },
] as const;

/** Framework choices of one card, resolved for the framework section. */
export function frameworksOfCard(cardId: string | undefined): readonly DeployFrameworkOption[] {
  return DEPLOY_APP_TYPE_CARDS.find((candidate) => candidate.id === cardId)?.frameworks ?? [];
}

/**
 * v3.2: auto-detect the framework from the application root's child
 * directories. Iterates the card's frameworks in display order (which doubles
 * as detection priority — decisive markers like `.dart_tool`/`unpackage` are
 * checked before weaker ones) and returns the first framework whose
 * {@link DeployFrameworkOption.detectDirectories} are all present. Undefined
 * when nothing matches: the caller keeps the card default.
 */
export function detectFrameworkId(
  frameworks: readonly DeployFrameworkOption[],
  childDirectories: readonly string[] | undefined,
): string | undefined {
  if (childDirectories === undefined || childDirectories.length === 0) return undefined;
  const children = new Set(childDirectories);
  // 无标记的框架（如 Kotlin/Swift 默认项）不参与匹配，仅作兜底默认值。
  return frameworks.find((framework) =>
    framework.detectDirectories !== undefined
    && framework.detectDirectories.length > 0
    && framework.detectDirectories.every((name) => children.has(name)),
  )?.id;
}

/** Card id + framework id → concrete resolution row (option). */
export function resolveDeployAppType(cardId: string | undefined, frameworkId?: string): DeployAppTypeOption | undefined {
  if (cardId === undefined) return undefined;
  const card = DEPLOY_APP_TYPE_CARDS.find((candidate) => candidate.id === cardId);
  if (card === undefined) return undefined;
  const framework = card.frameworks.find((candidate) => candidate.id === (frameworkId ?? card.defaultFrameworkId));
  if (framework === undefined) return undefined;
  return DEPLOY_APP_TYPE_OPTIONS.find((option) => option.id === framework.optionId);
}

/* ------------------------------------------------------------------ *
 * v4：项目规范驱动的可选性（发布对话框「支持 / 不支持」判定）
 * ------------------------------------------------------------------ */

/**
 * 一个应用类型卡片在当前项目下的可选性。
 *
 * 判定只做「可证伪」的排除：只有当我们确实读到了项目的 `apps/` 表面清单、
 * 且清单里没有该卡片需要的表面根时才判为不支持。清单不可读（例如目录已
 * 深入到某个表面根内部、或宿主无列举能力）时不判不支持，避免把用户锁死。
 */
export interface DeployAppTypeAvailability {
  readonly cardId: string
  /** 当前项目下该应用类型是否可发布。 */
  readonly supported: boolean
  /** 不支持原因（i18n 键）。 */
  readonly reasonKey?: import("../i18n.ts").PublishingMessageKey | undefined
  /** 不支持时缺失的规范 `apps/` 目录名，例如 `apps/sdkwork-im-pc`。 */
  readonly requiredDirectory?: string | undefined
}

/** 卡片需要的规范表面目录名（`apps/sdkwork-<code>-<suffix>`）。 */
export function requiredSurfaceDirectory(
  surface: AppSurfaceId,
  applicationCode: string | undefined,
): string | undefined {
  const suffix = APP_SURFACE_DIRECTORY_SUFFIX[surface];
  if (suffix === "") return undefined;
  return `apps/sdkwork-${applicationCode ?? "<code>"}-${suffix}`;
}

/**
 * 依据项目画像判定每张应用类型卡片的可选性。
 *
 * 非 sdkwork 项目、或尚未读到 `apps/` 表面清单的 sdkwork 项目：全部类型可选，
 * 由用户自行选择。sdkwork 项目且清单可读时，只放开项目实际提供表面所对应的
 * 卡片；无表面要求的卡片（API 服务 / 静态资源，发布仓库根目录）始终可选。
 */
export function classifyAppTypeCards(
  cards: readonly DeployAppTypeCard[],
  profile: DeployProjectProfile,
): readonly DeployAppTypeAvailability[] {
  const gate = profile.sdkwork && profile.surfaces.length > 0;
  return cards.map((card) => {
    if (!gate || card.surface === undefined || profile.surfaces.includes(card.surface)) {
      return { cardId: card.id, supported: true };
    }
    return {
      cardId: card.id,
      supported: false,
      reasonKey: "typeUnsupportedRequires",
      requiredDirectory: requiredSurfaceDirectory(card.surface, profile.applicationCode),
    };
  });
}

/**
 * 一个框架 / 架构选项与当前项目架构的关系。
 */
export interface DeployFrameworkAvailability {
  readonly frameworkId: string
  /** 项目表面的目录标记决定性命中该框架（对话框会自动选中）。 */
  readonly detected: boolean
  /**
   * 项目目录已经表明是别的架构：该框架自带标识目录，但它们在本项目中缺席。
   * 这类选项与项目架构不符，对话框禁止选择。
   */
  readonly conflicting: boolean
}

/**
 * v4：依据所选表面根的目录列举判定各框架是否与项目架构相符。
 *
 * 只有「另一个框架的决定性标记出现、而本框架的标识目录缺席」时才判冲突，
 * 因此无标识目录的兜底框架（Kotlin / Swift / React 等）永远不会被禁用，
 * 多架构共存（如 React Native 工程同时含 `android/`）也不会被误判。
 */
export function classifyFrameworks(
  frameworks: readonly DeployFrameworkOption[],
  childDirectories: readonly string[] | undefined,
): readonly DeployFrameworkAvailability[] {
  const children = childDirectories === undefined ? undefined : new Set(childDirectories);
  const decisive = detectFrameworkId(frameworks, childDirectories);
  return frameworks.map((framework) => {
    const markers = framework.detectDirectories ?? [];
    const matches = children !== undefined
      && markers.length > 0
      && markers.every((name) => children.has(name));
    return {
      frameworkId: framework.id,
      detected: decisive === framework.id,
      conflicting: decisive !== undefined
        && decisive !== framework.id
        && markers.length > 0
        && !matches,
    };
  });
}

/** Category selection stored in metadata. */
export interface DeployAppCategorySelection {
  readonly id: string
  readonly path: readonly { readonly id: string; readonly label: string }[]
}

/**
 * Drive-backed media reference stored in metadata.media.
 *
 * Optional members are spelled `?: T | undefined`: pixel dimensions are only
 * known after the browser image decoder has run, so `undefined` is a real
 * third state besides "absent", and `exactOptionalPropertyTypes` requires the
 * type to say so instead of forcing a cast at every assignment.
 */
export interface DeployAppMediaRef {
  readonly driveNodeId: string
  readonly driveSpaceId: string
  readonly uploadItemId: string
  readonly uploadSessionId: string
  readonly fileName: string
  readonly contentType: string
  readonly width?: number | undefined
  readonly height?: number | undefined
  readonly url?: string | undefined
}

/** Media group persisted to deploy_app.metadata.media. */
export interface DeployAppMediaGroup {
  readonly icon?: DeployAppMediaRef | undefined
  readonly cover?: DeployAppMediaRef | undefined
  readonly screenshots: Record<string, readonly DeployAppMediaRef[]>
}

/** Full create-deploy-app dialog model. */
export interface CreateDeployAppInput {
  /** 需求 1: 发布目录（sourceDirectory）。 */
  readonly sourceDirectory: string
  /** 需求 1: 关联已有应用；缺省时创建新应用。 */
  readonly associateAppId?: string | undefined
  /** 需求 1: 新应用名称（关联模式可空，由后端推导）。 */
  readonly name?: string | undefined
  readonly slug?: string | undefined
  /** 需求 2: 应用类型。 */
  readonly type: DeployAppTypeOption
  /** 需求 3: 多级分类。 */
  readonly category?: DeployAppCategorySelection | undefined
  /** 需求 4/5/6: 图标/封面/截图（Drive 上传后的引用）。 */
  readonly media?: DeployAppMediaGroup | undefined
  /** 需求 7: 版本号。 */
  readonly version: string
  /** 需求 8: 应用描述。 */
  readonly description?: string | undefined
  /** 需求 9: release notes。 */
  readonly releaseNotes?: string | undefined
  /** 可选：关联站点（静态/SPA 场景）。 */
  readonly siteId?: string | undefined
  /** v2: 发布目标环境（ENVIRONMENT_SPEC 规范环境）。 */
  readonly environment?: DeployEnvironmentId | undefined
  /** v2: 部署形态（standalone / cloud，profile id 前半）。 */
  readonly deploymentMode?: DeployDeploymentMode | undefined
  /** v2: 目录检测得到的 sdkwork 应用代码（sdkwork-<code>）。 */
  readonly applicationCode?: string | undefined
  /** v3: 选中的框架/架构 id（写入 metadata.framework）。 */
  readonly framework?: string | undefined
  /** v3: 构建产物目录（相对应用根，写入 metadata.buildOutputPath）。 */
  readonly buildOutputPath?: string | undefined
}

/** 需求 7: 语义化版本校验。 */
const SEMVER_PATTERN = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-((?:0|[1-9]\d*|\d*[a-zA-Z-][0-9a-zA-Z-]*)(?:\.(?:0|[1-9]\d*|\d*[a-zA-Z-][0-9a-zA-Z-]*))*))?(?:\+([0-9a-zA-Z-]+(?:\.[0-9a-zA-Z-]+)*))?$/;

export function isValidSemver(version: string): boolean {
  return SEMVER_PATTERN.test(version.trim())
}

/** Derive a slug from a name: lowercase, ascii, dash-separated. */
export function deriveAppSlug(name: string): string {
  const normalized = name
    .trim()
    .toLowerCase()
    .replace(/[\s_]+/g, "-")
    .replace(/[^a-z0-9-]/g, "")
    .replace(/-+/g, "-")
    .replace(/^-|-$/g, "")
  return normalized
}

/** One media upload request. */
export interface DeployAppMediaUpload {
  readonly kind: "icon" | "cover" | "screenshot"
  readonly file: DriveUploaderBlobLike
  readonly fileName: string
  readonly contentType: string
  /** Screenshot target key when kind === "screenshot". */
  readonly targetKey?: string | undefined
  readonly width?: number | undefined
  readonly height?: number | undefined
}

export interface DeployAppPublishingService {
  /** 需求 1: 可关联的已有应用列表。 */
  listApps(params?: {
    page?: number | undefined
    pageSize?: number | undefined
    keyword?: string | undefined
  }): Promise<{ items: AppResponse[]; pageInfo: PageInfo }>
  /** 需求 4/5/6: 上传媒体到 Drive，返回可持久化的引用。 */
  uploadMedia(input: DeployAppMediaUpload, appResourceId: string): Promise<DeployAppMediaRef>
  /** 需求 1/7/8/9: 创建（或更新元数据）应用并写平台目标。 */
  createApp(input: CreateDeployAppInput): Promise<AppResponse>
  /** 给已有应用追加平台目标。 */
  createPlatformTarget(appId: string, request: CreatePlatformTargetRequest): Promise<unknown>
  /** 组装 deploy_app.metadata JSONB。 */
  buildMetadata(input: CreateDeployAppInput): Record<string, unknown>
}

/** Service context: two generated clients + optional idempotency source. */
export interface DeployAppPublishingServiceOptions {
  readonly deployClient: SdkworkDeployAppClient
  readonly driveClient: SdkworkDriveAppClient
  readonly createIdempotencyKey?: () => string
}

/** Drive 上传结果 → 持久化引用（取 console-core artifacts 同一字段集）。 */
export function toDeployAppMediaRef(
  uploaded: DriveUploaderUploadResult,
  meta: Omit<DeployAppMediaRef, "driveNodeId" | "driveSpaceId" | "uploadItemId" | "uploadSessionId">,
): DeployAppMediaRef {
  return {
    driveNodeId: uploaded.uploadItem.nodeId,
    driveSpaceId: uploaded.uploadItem.spaceId,
    uploadItemId: uploaded.uploadItem.id,
    uploadSessionId: uploaded.uploadSession.id,
    fileName: meta.fileName,
    contentType: meta.contentType,
    width: meta.width,
    height: meta.height,
  }
}

export function createDeployAppPublishingService(
  options: DeployAppPublishingServiceOptions,
): DeployAppPublishingService {
  const createIdempotencyKey = options.createIdempotencyKey ?? (() => uuid())
  const { deployClient, driveClient } = options

  return {
    listApps(params) {
      return deployClient.app.list(
        params && {
          ...(params.page === undefined ? {} : { page: params.page }),
          ...(params.pageSize === undefined ? {} : { pageSize: params.pageSize }),
          ...(params.keyword === undefined ? {} : { keyword: params.keyword }),
        },
      )
    },

    async uploadMedia(input, appResourceId) {
      const uploaded = await driveClient.uploader.uploadArchive({
        file: input.file,
        appResourceType: "deploy.app.media",
        appResourceId,
        scene: `deploy-app-${input.kind}`,
        source: "@sdkwork/deployments-pc-console-publishing",
        originalFileName: input.fileName,
        contentType: input.contentType,
      })
      return toDeployAppMediaRef(uploaded, {
        fileName: input.fileName,
        contentType: input.contentType,
        width: input.width,
        height: input.height,
      })
    },

    buildMetadata(input) {
      const metadata: Record<string, unknown> = {
        sourceDirectory: input.sourceDirectory,
        version: input.version.trim(),
        releaseNotes: input.releaseNotes?.trim() || undefined,
        category: input.category
          ? { id: input.category.id, path: input.category.path }
          : undefined,
        media: input.media ?? undefined,
        // v2: 环境与目录检测结果一并落入 JSONB，后端 schema 无需迁移。
        environment: input.environment,
        deploymentMode: input.deploymentMode,
        applicationCode: input.applicationCode,
        surface: input.type.surface,
        // v3: 框架架构与构建产物相对路径（双路径模型：sourceDirectory=应用根）。
        framework: input.framework,
        buildOutputPath: input.buildOutputPath?.trim().replace(/[\\/]+$/, "") || undefined,
      }
      // Drop explicit undefined so JSONB stays tidy.
      return Object.fromEntries(
        Object.entries(metadata).filter(([, value]) => value !== undefined),
      )
    },

    async createApp(input) {
      const idempotencyKey = createIdempotencyKey()

      // 需求 1: 关联已有应用 → 仅更新元数据（版本/分类/媒体/说明）。
      if (input.associateAppId) {
        const name = input.name?.trim()
        const description = input.description?.trim()
        const updated = await deployClient.app.update(input.associateAppId, {
          // Generated request types keep `name?: string`, which
          // `exactOptionalPropertyTypes` will not satisfy with `string |
          // undefined`; hand-editing generated output is forbidden, so unset
          // members are omitted instead (the wire treats them identically).
          ...(name === undefined ? {} : { name }),
          ...(description === undefined ? {} : { description }),
          metadata: this.buildMetadata(input),
        })
        return updated
      }

      // 需求 1: 创建新应用。
      const name = input.name?.trim()
        ?? input.applicationCode
        ?? input.sourceDirectory.split(/[\\/]/).filter(Boolean).at(-1)
        ?? "Untitled app"
      const slug = input.slug?.trim() || deriveAppSlug(name) || undefined
      const description = input.description?.trim() || undefined
      const request: CreateAppRequest = {
        name,
        appKind: toSdkAppKind(input.type.appKind),
        metadata: this.buildMetadata(input),
        idempotencyKey,
        ...(slug === undefined ? {} : { slug }),
        ...(description === undefined ? {} : { description }),
        ...(input.siteId === undefined ? {} : { siteId: input.siteId }),
      }
      const created = await deployClient.app.create(request, { idempotencyKey })

      // 需求 2: 平台目标（deploy_app_platform_target）。
      await this.createPlatformTarget(created.id, {
        targetKey: input.type.id,
        platform: toSdkPlatform(input.type.platform),
        ...(input.type.techStack === undefined ? {} : { techStack: input.type.techStack }),
        idempotencyKey: createIdempotencyKey(),
      })
      return created
    },

    createPlatformTarget(appId, request) {
      return deployClient.app.platformTargets.create(appId, request, {
        idempotencyKey: request.idempotencyKey ?? createIdempotencyKey(),
      })
    },
  }
}

export type { AppStatus };
