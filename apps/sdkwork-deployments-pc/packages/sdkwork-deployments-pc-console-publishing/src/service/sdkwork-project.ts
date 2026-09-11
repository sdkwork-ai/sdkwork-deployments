/**
 * SdkworkProject — sdkwork 项目规范判定的通用类（发布对话框 v4 需求）。
 *
 * 设计约束（高内聚 / 低耦合）：
 *
 * - **零依赖**：本文件不 import 任何模块（既无 React，也无生成 SDK、无文件
 *   系统、无宿主端口）。所有输入都是调用方给的纯数据（目录名、路径、子目录
 *   名列表），因此它可以被控制台、BirdCoder 插件、CLI、服务端乃至测试直接
 *   服用；将来要搬到别的包（含无 UI 的领域包）只需整文件移动。
 * - **只描述规范，不描述 UI**：本类词汇完全来自 sdkwork-specs 的
 *   `APPLICATION_SPEC.md`（应用代码、应用表面目录、布局标记、目录推导），
 *   不含「应用类型卡片 / 构建产物 / 部署环境」等对话框概念——那些属于上层
 *   适配（见本包的 `project-detection.ts` / `deploy-app-publishing.ts`）。
 * - **只做可证伪的判定**：`apps/` 清单不可读时不假装知道有哪些表面；
 *   {@link SdkworkProject.isSdkworkProject} 与「表面清单」是两个独立信号，
 *   互不推断，上层据此决定是否收紧用户可选范围。
 *
 * Authorities:
 * - `APPLICATION_SPEC.md` §2 — `apps/` 表面根
 *   `apps/sdkwork-<application-code>-<client-arch>`，`<client-arch>` 取
 *   {@link SDKWORK_SURFACE_ARCHITECTURES}；`-common` 是共享包族根，不是表面。
 * - `APPLICATION_DEPLOY_LAYOUT_SPEC.md` §2 — 可部署根标记（`apps/`、`specs/`、
 *   `etc/`、`deployments/`、`.sdkwork/`）。
 */

/** sdkwork-specs `apps/` 应用表面架构后缀（APPLICATION_SPEC §2）。 */
export const SDKWORK_SURFACE_ARCHITECTURES = [
  "pc",
  "h5",
  "desktop",
  "mini-program",
  "android-mobile",
  "ios-mobile",
  "harmony-mobile",
  "flutter-mobile",
  "uniapp",
  "unity",
  "pad",
  "static-web",
] as const;

/** 应用表面架构后缀。 */
export type SdkworkSurfaceArchitecture = (typeof SDKWORK_SURFACE_ARCHITECTURES)[number];

/**
 * `apps/` 下不构成可发布表面的子目录后缀：`-common` 是跨架构共享包族根
 * （APPLICATION_SPEC §2），只提供包，不提供交付物。
 */
export const SDKWORK_NON_SURFACE_SUFFIXES: readonly string[] = ["common"];

/** 判定可部署根是否符合 sdkwork 布局规范所需的根目录标记。 */
export const SDKWORK_LAYOUT_MARKERS: readonly string[] = [
  "apps",
  "deployments",
  "etc",
  "specs",
  ".sdkwork",
];

/** 布局规范符合度：全部标记齐备 / 部分齐备 / 未识别。 */
export type SdkworkProjectConformance = "conformant" | "partial" | "unknown";

/** 一个检测到的应用表面根（`apps/sdkwork-<code>-<arch>/`）。 */
export interface SdkworkAppSurface {
  /** `apps/` 下的目录名，例如 `sdkwork-im-flutter-mobile`。 */
  readonly directory: string
  /** 解析出的架构后缀。 */
  readonly architecture: SdkworkSurfaceArchitecture
  /** 解析出的应用代码（`sdkwork-<code>-<arch>` 的 `<code>`）。 */
  readonly applicationCode: string
  /** 表面根绝对路径（由输入 rootPath 拼接，保留原分隔符风格）。 */
  readonly path: string
  /**
   * 表面根的子目录名，仅当宿主列举过该层时存在。框架 / 架构判定依赖它，
   * 缺失时上层应回退到默认架构而不是猜测。
   */
  readonly childDirectories?: readonly string[] | undefined
}

/** 判定输入：一个目录的列举结果（全部为纯数据，可选）。 */
export interface SdkworkProjectInput {
  /** 被检查目录的绝对路径。 */
  readonly rootPath?: string | undefined
  /**
   * 目录路径。无列举能力时的兜底信号；`rootPath` 缺省时也作为 rootPath 使用。
   */
  readonly directory?: string | undefined
  /** 被检查目录的直接子目录名。 */
  readonly childDirectories?: readonly string[] | undefined
  /** `<root>/apps` 的直接子目录名。 */
  readonly appsChildDirectories?: readonly string[] | undefined
  /** 各表面根的直接子目录名，键为 `apps/` 子目录名。 */
  readonly surfaceChildDirectories?: Readonly<Record<string, readonly string[]>> | undefined
}

/** 表面目录名解析结果。 */
export interface SdkworkSurfaceDirectoryName {
  readonly directory: string
  readonly applicationCode: string
  readonly architecture: SdkworkSurfaceArchitecture
}

/** `sdkwork-<code>`：仓库根目录名（无架构后缀）。 */
const REPOSITORY_DIRECTORY_PATTERN = /^sdkwork-[a-z0-9][a-z0-9-]*$/;

/**
 * 表面目录匹配式。架构后缀按长度倒序拼接，保证更长的架构名不会被它的前缀
 * 抢先匹配（`static-web` 之于 `static`，`android-mobile` 之于 `android`）。
 * 应用代码用非贪婪捕获，因此 `sdkwork-app-store-pc` 得到 `app-store`。
 */
const SURFACE_DIRECTORY_PATTERN = new RegExp(
  `^sdkwork-(?<code>[a-z0-9][a-z0-9-]*?)-(?<arch>${[...SDKWORK_SURFACE_ARCHITECTURES]
    .sort((left, right) => right.length - left.length)
    .join("|")})$`,
);

/** 路径分段（丢弃空段）。 */
function segmentsOf(directory: string): readonly string[] {
  return directory.split(/[\\/]/).filter((segment) => segment !== "");
}

/** 路径末段，空路径返回 undefined。 */
function basenameOf(directory: string | undefined): string | undefined {
  if (directory === undefined) return undefined;
  const segments = segmentsOf(directory);
  const basename = segments[segments.length - 1];
  return basename === undefined || basename === "" ? undefined : basename;
}

/** 目录使用的分隔符（含 `\` 即为 Windows 风格）。 */
function separatorOf(directory: string): string {
  return directory.includes("\\") ? "\\" : "/";
}

/** 用与 `directory` 相同的分隔符拼接分段，保留 POSIX/UNC 前导分隔符。 */
function joinSegments(directory: string, segments: readonly string[]): string {
  const separator = separatorOf(directory);
  const prefix = /^[\\/]/.test(directory) ? separator : "";
  return prefix + segments.join(separator);
}

/** 在 `parent` 下追加子段，去掉 parent 的尾部分隔符。 */
function appendSegments(parent: string, children: readonly string[]): string {
  const separator = separatorOf(parent);
  return `${parent.replace(/[\\/]+$/, "")}${separator}${children.join(separator)}`;
}

/** 架构规范顺序表（应用表面展示顺序）。 */
const SURFACE_ARCHITECTURE_RANK: Readonly<Record<SdkworkSurfaceArchitecture, number>> =
  Object.fromEntries(
    SDKWORK_SURFACE_ARCHITECTURES.map((architecture, index) => [architecture, index]),
  ) as Record<SdkworkSurfaceArchitecture, number>;

/** 内部状态（构造后不再变更）。 */
interface SdkworkProjectState {
  readonly rootPath?: string | undefined
  readonly directory?: string | undefined
  readonly childDirectories: readonly string[]
  readonly presentMarkers: readonly string[]
  readonly missingMarkers: readonly string[]
  readonly conformance: SdkworkProjectConformance
  readonly surfaces: readonly SdkworkAppSurface[]
  readonly applicationCode?: string | undefined
}

/**
 * sdkwork 项目判定的通用类：把「这个目录是不是 sdkwork 项目、它提供哪些应用
 * 表面、某个架构的发布源目录在哪」收敛成一个不可变值对象。
 *
 * ```ts
 * // 仅凭路径判断（宿主不给目录列举时）
 * SdkworkProject.isSdkworkDirectory('E:\\ws\\sdkwork-im');   // true
 * SdkworkProject.isSdkworkDirectory('apps/sdkwork-im-pc');   // true
 * SdkworkProject.isSdkworkDirectory('E:\\ws\\my-vite-app');  // false
 *
 * // 凭列举结果建模，再问「项目提供了哪些应用表面」
 * const project = SdkworkProject.inspect({
 *   rootPath: 'E:\\ws\\sdkwork-im',
 *   childDirectories: ['apps', 'specs', 'etc', 'deployments', '.sdkwork'],
 *   appsChildDirectories: ['sdkwork-im-pc', 'sdkwork-im-h5', 'sdkwork-im-common'],
 *   surfaceChildDirectories: { 'sdkwork-im-h5': ['src', 'dist'] },
 * });
 * project.isSdkworkProject;         // true
 * project.architectures;            // ['pc', 'h5']（-common 不算表面）
 * project.has('pc');                // true
 * project.childDirectoriesOf('h5'); // ['src', 'dist']
 * project.sourceDirectoryFor('h5'); // 'E:\\ws\\sdkwork-im\\apps\\sdkwork-im-h5'
 * ```
 *
 * 实例是不可变值对象：状态只在构造时写入，方法均为纯查询（无 IO、无副作用）。
 */
export class SdkworkProject {
  /** 可部署根标记（APPLICATION_DEPLOY_LAYOUT_SPEC §2）。 */
  static readonly layoutMarkers: readonly string[] = SDKWORK_LAYOUT_MARKERS;
  /** `apps/` 合法架构后缀（APPLICATION_SPEC §2）。 */
  static readonly surfaceArchitectures: readonly SdkworkSurfaceArchitecture[] = SDKWORK_SURFACE_ARCHITECTURES;
  /** `apps/` 下不属于可发布表面的后缀。 */
  static readonly nonSurfaceSuffixes: readonly string[] = SDKWORK_NON_SURFACE_SUFFIXES;
  /** `sdkwork-<code>` 仓库根目录名模式。 */
  static readonly repositoryDirectoryPattern: RegExp = REPOSITORY_DIRECTORY_PATTERN;

  /**
   * 解析一个 `apps/` 子目录名。
   * @returns 应用代码与架构后缀；非表面目录（含 `-common`）返回 undefined。
   */
  static parseSurfaceDirectoryName(name: string): SdkworkSurfaceDirectoryName | undefined {
    const groups = SURFACE_DIRECTORY_PATTERN.exec(name)?.groups;
    if (groups === undefined) return undefined;
    const architecture = groups.arch as SdkworkSurfaceArchitecture;
    if (SDKWORK_NON_SURFACE_SUFFIXES.includes(architecture)) return undefined;
    return { directory: name, applicationCode: groups.code as string, architecture };
  }

  /**
   * 仅凭目录名 / 路径判断是否为 sdkwork 仓库根或规范表面根，无需列举。
   *
   * 这是最轻量的通用判定，适用于宿主不提供目录列举的场景（例如只知道会话
   * cwd）。返回 true 只说明命名符合规范，**不**代表 `apps/` 清单可读。
   */
  static isSdkworkDirectory(directory: string | undefined): boolean {
    const basename = basenameOf(directory);
    if (basename === undefined) return false;
    return REPOSITORY_DIRECTORY_PATTERN.test(basename)
      || SdkworkProject.parseSurfaceDirectoryName(basename) !== undefined;
  }

  /** 从目录名解析 `sdkwork-<code>` 应用代码（仓库根或表面根均可）。 */
  static applicationCodeOfDirectory(directory: string | undefined): string | undefined {
    const basename = basenameOf(directory);
    if (basename === undefined) return undefined;
    const surface = SdkworkProject.parseSurfaceDirectoryName(basename);
    if (surface !== undefined) return surface.applicationCode;
    if (REPOSITORY_DIRECTORY_PATTERN.test(basename)) return basename.slice("sdkwork-".length);
    return undefined;
  }

  /**
   * 从路径本身推导某个架构的规范表面根（不依赖宿主列举）：
   *
   * - 当前目录已是该架构的规范表面根 → undefined（无需完善）；
   * - 当前目录是 `apps/` 下另一个表面根 → 推导同级表面根；
   * - 当前目录是 `sdkwork-<code>` 仓库根 → `<dir>/apps/sdkwork-<code>-<arch>`；
   * - 其他目录 → undefined（绝不臆造路径）。
   *
   * 结果保留原路径的分隔符风格。
   */
  static deriveSurfaceDirectory(
    directory: string,
    architecture: SdkworkSurfaceArchitecture,
  ): string | undefined {
    const segments = segmentsOf(directory);
    const basename = segments[segments.length - 1];
    if (basename === undefined) return undefined;

    const current = SdkworkProject.parseSurfaceDirectoryName(basename);
    if (current !== undefined) {
      if (current.architecture === architecture) return undefined;
      // 同级表面目录切换：仅当父目录是 apps/（规范布局）时推导兄弟表面根。
      if (segments[segments.length - 2] !== "apps") return undefined;
      const sibling = `sdkwork-${current.applicationCode}-${architecture}`;
      return joinSegments(directory, [...segments.slice(0, -1), sibling]);
    }

    if (!REPOSITORY_DIRECTORY_PATTERN.test(basename)) return undefined;
    const applicationCode = basename.slice("sdkwork-".length);
    return joinSegments(directory, [...segments, "apps", `sdkwork-${applicationCode}-${architecture}`]);
  }

  /**
   * 由目录列举结果建模。输入为空、或宿主未列举时退化为「仅命名判定」的实例：
   * 此时 {@link surfaces} 为空、{@link conformance} 为 `unknown`，
   * {@link isSdkworkProject} 只反映目录名信号。
   */
  static inspect(input: SdkworkProjectInput | undefined): SdkworkProject {
    const rootPath = input?.rootPath ?? input?.directory;
    const childDirectories = [...(input?.childDirectories ?? [])];
    const surfaceChildDirectories = input?.surfaceChildDirectories;

    const presentMarkers = SDKWORK_LAYOUT_MARKERS.filter((marker) => childDirectories.includes(marker));
    const missingMarkers = SDKWORK_LAYOUT_MARKERS.filter((marker) => !childDirectories.includes(marker));
    // 无列举（空数组）→ unknown：不能把「宿主没给数据」当成「不规范」。
    const conformance: SdkworkProjectConformance = childDirectories.length === 0
      ? "unknown"
      : presentMarkers.length === SDKWORK_LAYOUT_MARKERS.length
        ? "conformant"
        : presentMarkers.length >= 2
          ? "partial"
          : "unknown";

    const surfaces: SdkworkAppSurface[] = [];
    const applicationCodes = new Set<string>();
    for (const name of input?.appsChildDirectories ?? []) {
      const parsed = SdkworkProject.parseSurfaceDirectoryName(name);
      if (parsed === undefined) continue;
      applicationCodes.add(parsed.applicationCode);
      surfaces.push({
        directory: name,
        architecture: parsed.architecture,
        applicationCode: parsed.applicationCode,
        path: rootPath === undefined ? name : appendSegments(rootPath, ["apps", name]),
        childDirectories: surfaceChildDirectories?.[name],
      });
    }
    surfaces.sort(
      (left, right) => SURFACE_ARCHITECTURE_RANK[left.architecture] - SURFACE_ARCHITECTURE_RANK[right.architecture],
    );

    const agreedCode = applicationCodes.size === 1 ? applicationCodes.values().next().value : undefined;
    return new SdkworkProject({
      rootPath,
      directory: input?.directory,
      childDirectories,
      presentMarkers,
      missingMarkers,
      conformance,
      surfaces,
      applicationCode: agreedCode ?? SdkworkProject.applicationCodeOfDirectory(rootPath),
    });
  }

  /**
   * 仅依据目录路径建模（无列举）。等价于 `inspect({ directory })`。
   */
  static ofDirectory(directory: string | undefined): SdkworkProject {
    return SdkworkProject.inspect({ directory });
  }

  /**
   * 目录位于 `apps/<surface>/` 内部时，其所属仓库根（`apps/` 的父目录）。
   *
   * 用途：宿主对表面根本身的列举看不到**同级**表面（那里没有 `apps/` 子目录），
   * 于是「本项目支持哪些应用」会退化成「无法证明」。补一次仓库根列举即可恢复
   * 完整表面清单。非 `apps/` 下的表面目录（不符规范布局）返回 undefined。
   */
  static repositoryRootOfDirectory(directory: string | undefined): string | undefined {
    if (directory === undefined) return undefined;
    const segments = segmentsOf(directory);
    if (segments.length < 3) return undefined;
    const basename = segments[segments.length - 1];
    if (basename === undefined) return undefined;
    if (segments[segments.length - 2] !== "apps") return undefined;
    if (SdkworkProject.parseSurfaceDirectoryName(basename) === undefined) return undefined;
    return joinSegments(directory, segments.slice(0, -2));
  }

  private constructor(private readonly state: SdkworkProjectState) {}

  /** 被检查目录的绝对路径（未知时为 undefined）。 */
  get rootPath(): string | undefined {
    return this.state.rootPath;
  }

  /**
   * 是否为 sdkwork 项目：命名符合规范（仓库根 / 表面根），或列举结果给出了
   * sdkwork 布局信号（存在表面根，或 ≥2 个布局标记）。
   *
   * 该信号代表「这是 sdkwork 项目」；是否知道它提供哪些表面另见
   * {@link hasSurfaceListing}。
   */
  get isSdkworkProject(): boolean {
    return this.state.surfaces.length > 0
      || this.state.conformance !== "unknown"
      || SdkworkProject.isSdkworkDirectory(this.state.directory ?? this.state.rootPath);
  }

  /** 布局规范符合度（无列举时为 `unknown`）。 */
  get conformance(): SdkworkProjectConformance {
    return this.state.conformance;
  }

  /** 应用代码：表面根一致推导优先，其次从目录名推导。 */
  get applicationCode(): string | undefined {
    return this.state.applicationCode;
  }

  /** 检测到的表面根，按规范架构顺序排列。 */
  get surfaces(): readonly SdkworkAppSurface[] {
    return this.state.surfaces;
  }

  /** 检测到的架构后缀集合（去重、按规范顺序）。 */
  get architectures(): readonly SdkworkSurfaceArchitecture[] {
    const found = new Set(this.state.surfaces.map((surface) => surface.architecture));
    return SDKWORK_SURFACE_ARCHITECTURES.filter((architecture) => found.has(architecture));
  }

  /** 已知的布局标记。 */
  get presentMarkers(): readonly string[] {
    return this.state.presentMarkers;
  }

  /** 缺失的布局标记。 */
  get missingMarkers(): readonly string[] {
    return this.state.missingMarkers;
  }

  /** 被检查根的直接子目录名。 */
  get childDirectories(): readonly string[] {
    return this.state.childDirectories;
  }

  /**
   * 是否读到了 `apps/` 表面清单。`false` 表示「无法证明项目有哪些表面」，
   * 上层据此**不应**收紧用户可选范围（只做可证伪的排除）。
   */
  get hasSurfaceListing(): boolean {
    return this.state.surfaces.length > 0;
  }

  /** 项目是否提供了该架构的表面根。 */
  has(architecture: SdkworkSurfaceArchitecture): boolean {
    return this.state.surfaces.some((surface) => surface.architecture === architecture);
  }

  /**
   * 该架构对应的表面根。表面清单已按规范顺序排序，因此跨端根（如
   * `-uniapp`）之外若另有专有根（如 `-h5`），取到的即是规范顺序里更靠前、
   * 更专有的那个发布源。
   */
  find(architecture: SdkworkSurfaceArchitecture): SdkworkAppSurface | undefined {
    return this.state.surfaces.find((surface) => surface.architecture === architecture);
  }

  /**
   * 供框架 / 构建产物判定使用的子目录名列表。`architecture` 缺省表示「发布
   * 仓库根目录」的架构（API 服务 / 静态资源），此时回退到被检查根本身的子
   * 目录名；无数据时返回 undefined（判定方应回退默认值而非猜测）。
   */
  childDirectoriesOf(
    architecture: SdkworkSurfaceArchitecture | undefined,
  ): readonly string[] | undefined {
    if (architecture !== undefined) return this.find(architecture)?.childDirectories;
    return this.state.childDirectories.length > 0 ? this.state.childDirectories : undefined;
  }

  /** 发布源目录：命中表面根用表面根，否则用被检查根 / 兜底路径。 */
  sourceDirectoryFor(
    architecture: SdkworkSurfaceArchitecture | undefined,
    fallback?: string | undefined,
  ): string {
    if (architecture !== undefined) {
      const surface = this.find(architecture);
      if (surface !== undefined) return surface.path;
    }
    return this.state.rootPath ?? fallback ?? "";
  }
}
