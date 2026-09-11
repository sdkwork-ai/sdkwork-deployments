/**
 * 发布对话框的项目目录检测适配层（v2 自动检测 / v3 双路径 / v3.4 环境联动 /
 * v4 应用类型支持度）。
 *
 * 分工（高内聚低耦合）：
 * - 「这个目录是不是 sdkwork 项目、提供哪些应用表面」的**规范判定**统一收敛
 *   到零依赖通用类 {@link SdkworkProject}（`./sdkwork-project.ts`），本文件
 *   不再自己实现正则与标记表；
 * - 本文件只做**对话框适配**：把通用类的结论翻译成对话框词汇——部署环境
 *   profile、`dist/<mode>/<env>` 产物布局、对话框应用表面 `AppSurfaceId`
 *   （与上层应用类型卡片一一对应），以及宿主列举载荷的解析。
 *
 * Host contract: the dialog stays decoupled from the filesystem — a host port
 * hands over child directory names (BirdCoder desktop via
 * uiWorkspace.listDirectory, or any future bridge) and this module maps them
 * onto the sdkwork-specs layout.
 */

import {
  SdkworkProject,
  type SdkworkProjectConformance,
  type SdkworkSurfaceArchitecture,
} from "./sdkwork-project.ts";

/** Canonical publish environment (ENVIRONMENT_SPEC.md §2; no aliases). */
export type DeployEnvironmentId = "development" | "test" | "staging" | "demo" | "production";

/** Deployment mode half of the canonical profile id (ENVIRONMENT_SPEC.md §5.1). */
export type DeployDeploymentMode = "standalone" | "cloud";

/** Command/operator aliases normalized before canonical profile selection. */
export const DEPLOY_ENVIRONMENT_ALIASES: Readonly<Record<string, DeployEnvironmentId>> = {
  dev: "development",
  prod: "production",
};

/** Canonical environments in publish-target order. */
export const DEPLOY_ENVIRONMENT_IDS: readonly DeployEnvironmentId[] = [
  "development",
  "test",
  "staging",
  "demo",
  "production",
];

/** Deployment modes in dialog order. */
export const DEPLOY_DEPLOYMENT_MODES: readonly DeployDeploymentMode[] = ["standalone", "cloud"];

/** Canonical profile id `<mode>.<environment>` (ENVIRONMENT_SPEC.md §5.1). */
export function deployProfileId(mode: DeployDeploymentMode, environment: DeployEnvironmentId): string {
  return `${mode}.${environment}`;
}

/**
 * Dist directory segment aliases for browser builds (FRONTEND_CODE_SPEC.md §7,
 * mirrored from sdkwork-specs tools/browser-dist-layout.mjs). The spec alias
 * table covers the four lifecycle environments; the dialog additionally offers
 * `demo`, which builds from the staging subtree (demo is a staging-shaped
 * preview target, and the spec tool rejects unknown aliases).
 */
export const BROWSER_DIST_ENV_ALIASES: Readonly<Record<DeployEnvironmentId, string>> = {
  development: "dev",
  test: "test",
  staging: "staging",
  demo: "staging",
  production: "prod",
};

/**
 * v3.4: should the environment-driven build-output sync apply over the
 * current field value? The sync only overrides machine-owned values —
 * empty, the framework's default build output, or the last auto-filled
 * value. A manually customized path is never overwritten.
 */
export function shouldSyncEnvironmentBuildOutput(
  current: string,
  frameworkDefault: string | undefined,
  lastAutoValue: string | undefined,
): boolean {
  const trimmed = current.trim();
  if (trimmed === "") return true;
  if (frameworkDefault !== undefined && trimmed === frameworkDefault) return true;
  return lastAutoValue !== undefined && trimmed === lastAutoValue;
}

/**
 * v3.4: relative build-output directory for one browser application root
 * (surfaces pc/h5/static): `dist/<deploymentProfile>/<envAlias>` — e.g.
 * `dist/standalone/dev`, `dist/cloud/prod`. Every deployment profile owns its
 * own environment subtree so standalone and cloud builds coexist; a bare
 * `dist/` is never a valid build output (APP_CLIENT_ARCHITECTURE_ALIGNMENT_SPEC.md
 * §2.2 / ENVIRONMENT_SPEC.md §5.1).
 */
export function browserDistOutputPath(mode: DeployDeploymentMode, environment: DeployEnvironmentId): string {
  return `dist/${mode}/${BROWSER_DIST_ENV_ALIASES[environment]}`;
}

/** Normalize a legacy alias; unknown values pass through unchanged. */
export function canonicalEnvironment(value: string): string {
  return DEPLOY_ENVIRONMENT_ALIASES[value.trim().toLowerCase()] ?? value.trim();
}

/**
 * App surface ids used by the dialog's type cards. `android`/`ios` are the
 * card-level ids; their `apps/` directory suffixes differ (see
 * {@link APP_SURFACE_DIRECTORY_SUFFIX}).
 */
export type AppSurfaceId =
  | "pc"
  | "h5"
  | "desktop"
  | "mini-program"
  | "android"
  | "ios"
  | "harmony"
  | "api"
  | "static";

/**
 * 对话框表面 → 该表面在 `apps/` 下使用的规范架构后缀
 * （APPLICATION_SPEC §2）。空串表示该表面没有专有 `apps/` 根：API 服务与
 * 静态资源发布仓库根目录本身。
 */
export const APP_SURFACE_DIRECTORY_SUFFIX: Readonly<Record<AppSurfaceId, string>> = {
  pc: "pc",
  h5: "h5",
  desktop: "desktop",
  "mini-program": "mini-program",
  android: "android-mobile",
  ios: "ios-mobile",
  harmony: "harmony-mobile",
  api: "",
  static: "",
};

/**
 * `apps/` 表面架构后缀 → 该表面根可承载的对话框应用表面。
 *
 * 一个表面根可以交付多个发布目标：`-flutter-mobile` 同时产出 Android 与 iOS；
 * `-uniapp` 是跨端根；`-unity` / `-pad` 按移动端类型发布。`-common` 与其它
 * 共享包族根不在表中（它们不是可发布表面，由 `SdkworkProject` 过滤）。
 *
 * 注意方向性差异：`-static-web` 是规范里真实的表面根，因此它会「点亮」对话框
 * 的「静态资源」类型；但该类型自身的规范发布目录仍是仓库根（见
 * {@link APP_SURFACE_DIRECTORY_SUFFIX} 中 `static` 为空串），二者不矛盾。
 */
export const APP_SURFACE_DIRECTORY_CAPABILITIES: Readonly<Record<string, readonly AppSurfaceId[]>> = {
  pc: ["pc"],
  h5: ["h5"],
  desktop: ["desktop"],
  "mini-program": ["mini-program"],
  "android-mobile": ["android"],
  "ios-mobile": ["ios"],
  "harmony-mobile": ["harmony"],
  "flutter-mobile": ["android", "ios"],
  uniapp: ["h5", "mini-program", "android", "ios", "harmony"],
  unity: ["android", "ios"],
  pad: ["android", "ios", "harmony"],
  "static-web": ["static"],
};

/** 对话框表面的规范展示顺序（表面徽标 / 摘要共用）。 */
const DIALOG_SURFACE_ORDER: readonly AppSurfaceId[] = [
  "pc", "h5", "desktop", "mini-program", "android", "ios", "harmony", "static", "api",
];

/**
 * 对话框表面 → 用于**推导**规范表面根时使用的架构后缀。只有拥有专有
 * `apps/` 根的表面在表中：API 服务与静态资源发布仓库根目录本身
 * （`APP_SURFACE_DIRECTORY_SUFFIX` 为空串），因此推导结果恒为 undefined。
 */
const SURFACE_ARCHITECTURE: Readonly<Record<"pc" | "h5" | "desktop" | "mini-program" | "android" | "ios" | "harmony", SdkworkSurfaceArchitecture>> = {
  pc: "pc",
  h5: "h5",
  desktop: "desktop",
  "mini-program": "mini-program",
  android: "android-mobile",
  ios: "ios-mobile",
  harmony: "harmony-mobile",
};

/** Host inspection payload: names only, no file contents required. */
export interface DeployProjectInspection {
  /** Absolute inspected directory path. */
  readonly rootPath: string
  /** Child directory names of the inspected root. */
  readonly childDirectories: readonly string[]
  /** Child directory names of `<root>/apps`, when that directory exists. */
  readonly appsChildDirectories?: readonly string[] | undefined
  /**
   * v3: child directory names of each `apps/` surface root, keyed by the
   * surface directory name. Optional: hosts without a deeper listing simply
   * omit it and the build-output detection falls back to framework defaults.
   */
  readonly surfaceChildDirectories?: Readonly<Record<string, readonly string[]>> | undefined
}

/** One detected `apps/sdkwork-<code>-<suffix>/` surface root. */
export interface DeployDetectedSurface {
  /**
   * Dialog surfaces this app root can publish, in canonical order. Usually a
   * single surface; cross-platform roots list several (`-flutter-mobile` →
   * `android` + `ios`).
   */
  readonly surfaces: readonly AppSurfaceId[]
  /** Matched `apps/` directory name. */
  readonly directory: string
  /** Spec architecture suffix of {@link directory} (`pc`, `flutter-mobile`, …). */
  readonly directorySuffix: SdkworkSurfaceArchitecture
  /** Absolute surface root (`<rootPath>/apps/<directory>`). */
  readonly path: string
  /** v3: child directory names of this surface root, when the host listed them. */
  readonly childDirectories?: readonly string[] | undefined
}

/** Layout conformance level reported to the dialog. */
export type DeployProjectConformance = SdkworkProjectConformance;

/** Detection result consumed by the directory step. */
export interface DeployProjectDetection {
  /** `sdkwork-<code>` application code derived from the matched surfaces. */
  readonly applicationCode?: string | undefined
  /** Detected surface roots, ordered by architecture. */
  readonly surfaces: readonly DeployDetectedSurface[]
  readonly conformance: DeployProjectConformance
  /** Spec markers present at the root. */
  readonly presentMarkers: readonly string[]
  /** Spec markers missing at the root. */
  readonly missingMarkers: readonly string[]
  /**
   * v4: child directory names of the inspected root itself. Root-publishing
   * types (`api` / `static`) use this to detect their framework and build
   * output, since they own no `apps/` surface root.
   */
  readonly rootChildDirectories: readonly string[]
  /** v4: 规范判定实例（通用类），供上层直接复用。 */
  readonly project: SdkworkProject
}

/** @returns the dialog surfaces an `apps/` child name can publish, or undefined. */
export function surfacesOfDirectoryName(
  name: string,
): { surfaces: readonly AppSurfaceId[]; directorySuffix: SdkworkSurfaceArchitecture; applicationCode: string } | undefined {
  const parsed = SdkworkProject.parseSurfaceDirectoryName(name);
  if (parsed === undefined) return undefined;
  const surfaces = APP_SURFACE_DIRECTORY_CAPABILITIES[parsed.architecture];
  if (surfaces === undefined || surfaces.length === 0) return undefined;
  return { surfaces, directorySuffix: parsed.architecture, applicationCode: parsed.applicationCode };
}

/**
 * 把宿主列举载荷翻译成对话框检测结果。规范判定（标记、表面、应用代码）
 * 全部委托给 {@link SdkworkProject}，这里只负责映射到对话框表面词汇。
 *
 * `sdkwork.app.config.json` 是文件且刻意不在此校验 —— 只看目录名的宿主桥
 * 观察不到它，该清单校验留给发布时的后端。
 */
export function detectSdkworkProject(inspection: DeployProjectInspection): DeployProjectDetection {
  const project = SdkworkProject.inspect({
    rootPath: inspection.rootPath,
    childDirectories: inspection.childDirectories,
    appsChildDirectories: inspection.appsChildDirectories,
    surfaceChildDirectories: inspection.surfaceChildDirectories,
  });

  const surfaces: DeployDetectedSurface[] = project.surfaces.map((surface) => ({
    surfaces: APP_SURFACE_DIRECTORY_CAPABILITIES[surface.architecture] ?? [],
    directory: surface.directory,
    directorySuffix: surface.architecture,
    path: surface.path,
    childDirectories: surface.childDirectories,
  }));

  return {
    applicationCode: project.applicationCode,
    surfaces,
    conformance: project.conformance,
    presentMarkers: project.presentMarkers,
    missingMarkers: project.missingMarkers,
    rootChildDirectories: [...inspection.childDirectories],
    project,
  };
}

/**
 * The detected surface root serving one publish surface, if any. A dedicated
 * root always wins over a cross-platform root (`-android-mobile` beats
 * `-flutter-mobile` for Android) so the dialog never points at a broader root
 * when the spec-compliant one exists.
 */
export function findDetectedSurface(
  detection: DeployProjectDetection | undefined,
  surface: AppSurfaceId | undefined,
): DeployDetectedSurface | undefined {
  if (detection === undefined || surface === undefined) return undefined;
  const candidates = detection.surfaces.filter((candidate) => candidate.surfaces.includes(surface));
  return candidates.find((candidate) => candidate.surfaces.length === 1) ?? candidates[0];
}

/**
 * Resolve the publish source directory for a selected surface: the detected
 * `apps/` surface root when present, otherwise the inspected root.
 */
export function resolveSourceDirectory(
  detection: DeployProjectDetection,
  surface: AppSurfaceId | undefined,
  rootPath: string,
): string {
  if (surface === undefined) return rootPath;
  return findDetectedSurface(detection, surface)?.path ?? rootPath;
}

/**
 * v4: 项目实际提供的对话框表面（由检测到的 `apps/` 表面根展开、去重、按
 * 规范顺序）。清单未读到时为 empty —— 调用方不得据此判定「不支持」。
 */
export function detectedSurfaceIds(
  detection: DeployProjectDetection | undefined,
): readonly AppSurfaceId[] {
  if (detection === undefined) return [];
  const found = new Set<AppSurfaceId>();
  for (const surface of detection.surfaces) {
    for (const id of surface.surfaces) found.add(id);
  }
  return DIALOG_SURFACE_ORDER.filter((id) => found.has(id));
}

/**
 * v4: 目录位于 `apps/<surface>/` 内部时，其所属仓库根路径。
 *
 * 宿主对表面根本身的列举看不到同级表面（那里没有 `apps/` 子目录），因此对话框
 * 需要补一次仓库根列举，才能正确回答「本项目支持哪些应用类型」。非 `apps/` 下
 * 的表面目录返回 undefined。委派给 {@link SdkworkProject.repositoryRootOfDirectory}。
 */
export function repositoryRootOf(directory: string | undefined): string | undefined {
  return SdkworkProject.repositoryRootOfDirectory(directory);
}

/**
 * v4: 项目规范画像 —— 驱动对话框「支持 / 不支持」判定的唯一输入。
 *
 * 由通用类 {@link SdkworkProject} 派生：`sdkwork` 表示这个是 sdkwork 项目，
 * `surfaces` 只列出宿主**观察到的**表面。`surfaces` 为空时上层必须保持全部
 * 应用类型可选（只做可证伪的排除）。
 */
export interface DeployProjectProfile {
  /** 底层规范判定实例（零依赖通用类）。 */
  readonly project: SdkworkProject
  /** 识别为 sdkwork 应用仓库（应用类型按 `apps/` 表面约束的前提）。 */
  readonly sdkwork: boolean
  /** 观察到的对话框表面，按规范顺序（可能为空）。 */
  readonly surfaces: readonly AppSurfaceId[]
  /** `sdkwork-<code>` 应用代码，已知时给出。 */
  readonly applicationCode?: string | undefined
}

/**
 * 由检测结果 + 当前目录构造项目画像。
 *
 * `directory` 单独传入是必要的：当用户走进某个表面根内部时，`apps/` 清单
 * 不可读（表面为空），但目录名本身仍然能证明「这是 sdkwork 项目」。
 */
export function projectProfile(
  detection: DeployProjectDetection | undefined,
  directory: string | undefined,
): DeployProjectProfile {
  const project = detection?.project ?? SdkworkProject.ofDirectory(directory);
  return {
    project,
    sdkwork: project.isSdkworkProject,
    surfaces: detectedSurfaceIds(detection),
    applicationCode: project.applicationCode,
  };
}

/**
 * v4.3: derive the spec-compliant surface root from the directory path alone
 * (no host listing required), in dialog surface vocabulary. Pure path
 * derivation is delegated to {@link SdkworkProject.deriveSurfaceDirectory};
 * `api`/`static` publish the repository root and therefore have no surface
 * directory → undefined.
 */
export function deriveSurfaceDirectory(
  directory: string,
  surface: AppSurfaceId,
): string | undefined {
  const architecture = SURFACE_ARCHITECTURE[surface as keyof typeof SURFACE_ARCHITECTURE];
  if (architecture === undefined) return undefined;
  return SdkworkProject.deriveSurfaceDirectory(directory, architecture);
}

/**
 * v3: directory names recognized as build-output roots when they appear as
 * direct children of the application root. Intentionally generic — the
 * framework-aware defaults live with the framework registry
 * (`deploy-app-publishing.ts`); this list only backs the "generic candidates"
 * chips shown next to the build-output field.
 */
export const KNOWN_BUILD_OUTPUT_DIRECTORY_NAMES: readonly string[] = [
  "dist", "build", "out", ".output", ".next", ".nuxt", "public", "unpackage",
  "target", "bin", "release",
];

/**
 * v3: does a relative build-output path exist under the surface root, judged
 * from the host's directory listing? `"."` (publish the root itself) always
 * exists; nested paths are judged by their first segment.
 */
export function buildOutputExists(
  buildOutputPath: string,
  childDirectories: readonly string[] | undefined,
): boolean | undefined {
  const trimmed = buildOutputPath.trim().replace(/^\.\//, "");
  if (trimmed === "" || trimmed === ".") return true;
  if (trimmed.startsWith("/") || trimmed.startsWith("../") || /^[a-zA-Z]:/.test(trimmed)) return false;
  if (childDirectories === undefined) return undefined;
  const firstSegment = trimmed.split(/[\\/]/)[0] ?? "";
  return childDirectories.includes(firstSegment);
}

/**
 * v3: generic build-output candidates observed in the surface root's child
 * directories (intersected with {@link KNOWN_BUILD_OUTPUT_DIRECTORY_NAMES}).
 */
export function detectBuildOutputCandidates(
  childDirectories: readonly string[] | undefined,
): readonly string[] {
  if (childDirectories === undefined) return [];
  return KNOWN_BUILD_OUTPUT_DIRECTORY_NAMES.filter((name) => childDirectories.includes(name));
}

/** POSIX/Windows-agnostic join for display paths (no filesystem access). */
export function joinPath(...segments: readonly string[]): string {
  return segments
    .filter((segment) => segment !== "")
    .map((segment, index) => (index === 0 ? segment.replace(/[\\/]+$/, "") : segment.replace(/^[\\/]+|[\\/]+$/g, "")))
    .join("/");
}
