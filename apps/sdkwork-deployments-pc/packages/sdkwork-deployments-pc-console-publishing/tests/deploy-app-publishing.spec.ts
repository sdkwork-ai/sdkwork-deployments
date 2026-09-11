/**
 * Unit tests for the create-deploy-app publishing service: slug derivation,
 * semver validation, metadata assembly (JSONB shape), Drive upload result
 * mapping, and the v4 project-conformance availability classifiers
 * (supported / unsupported application types, architecture-compatible
 * frameworks). Pure functions only — no clients are exercised here.
 */
import { describe, expect, it } from "vitest";
import {
  classifyAppTypeCards,
  classifyFrameworks,
  createDeployAppPublishingService,
  detectFrameworkId,
  DEPLOY_APP_TYPE_CARDS,
  DEPLOY_APP_TYPE_OPTIONS,
  deriveAppSlug,
  frameworksOfCard,
  isValidSemver,
  requiredSurfaceDirectory,
  resolveDeployAppType,
  toDeployAppMediaRef,
  type CreateDeployAppInput,
  type DeployAppTypeOption,
} from "../src/service/deploy-app-publishing.ts";
import {
  APP_SURFACE_DIRECTORY_SUFFIX,
  detectSdkworkProject,
  projectProfile,
  resolveSourceDirectory,
  type AppSurfaceId,
  type DeployProjectProfile,
} from "../src/service/project-detection.ts";

/** 一个 sdkwork 项目的对话框侧画像（用规范后缀构造 apps/ 表面目录）。 */
function sdkworkProfile(
  surfaces: readonly AppSurfaceId[],
  applicationCode: string,
): DeployProjectProfile {
  const detection = detectSdkworkProject({
    rootPath: `E:\\ws\\sdkwork-${applicationCode}`,
    childDirectories: ["apps", "deployments", "etc", "specs", ".sdkwork"],
    appsChildDirectories: surfaces.map(
      (surface) => `sdkwork-${applicationCode}-${APP_SURFACE_DIRECTORY_SUFFIX[surface]}`,
    ),
  });
  return projectProfile(detection, `E:\\ws\\sdkwork-${applicationCode}`);
}

/** 直接按 apps/ 目录名构造画像（用于跨端表面根等需要精确目录名的场景）。 */
function profileOfDirectories(directories: readonly string[], applicationCode = "im"): DeployProjectProfile {
  const detection = detectSdkworkProject({
    rootPath: `E:\\ws\\sdkwork-${applicationCode}`,
    childDirectories: ["apps", "deployments", "etc", "specs", ".sdkwork"],
    appsChildDirectories: directories,
  });
  return projectProfile(detection, `E:\\ws\\sdkwork-${applicationCode}`);
}

const flutterIos = DEPLOY_APP_TYPE_OPTIONS.find((option) => option.id === "flutter-ios");
const staticWeb = DEPLOY_APP_TYPE_OPTIONS.find((option) => option.id === "static-web");

describe("deriveAppSlug", () => {
  it("derives a lowercase dashed ascii slug from a display name", () => {
    expect(deriveAppSlug("My Store App")).toBe("my-store-app");
  });

  it("drops non-ascii characters and collapses separators", () => {
    expect(deriveAppSlug("商城 App__v1")).toBe("app-v1");
  });

  it("trims leading/trailing separators", () => {
    expect(deriveAppSlug("- App -")).toBe("app");
  });
});

describe("isValidSemver", () => {
  it("accepts plain and prerelease semvers", () => {
    expect(isValidSemver("1.0.0")).toBe(true);
    expect(isValidSemver("1.2.3-beta.1")).toBe(true);
    expect(isValidSemver("0.0.1+build.7")).toBe(true);
  });

  it("rejects malformed versions", () => {
    expect(isValidSemver("1.0")).toBe(false);
    expect(isValidSemver("v1.0.0")).toBe(false);
    expect(isValidSemver("1.0.0.0")).toBe(false);
    expect(isValidSemver("")).toBe(false);
  });
});

describe("createDeployAppPublishingService metadata assembly", () => {
  it("assembles deploy_app.metadata JSONB with the dialog fields", () => {
    const input: CreateDeployAppInput = {
      sourceDirectory: "/workspace/my-app",
      type: flutterIos as DeployAppTypeOption,
      version: "1.0.0",
      description: "A test app",
      releaseNotes: "First release",
      category: {
        id: "dev-tools",
        path: [{ id: "developer", label: "开发者" }, { id: "dev-tools", label: "开发工具" }],
      },
      media: {
        icon: { driveNodeId: "n1", driveSpaceId: "s1", uploadItemId: "i1", uploadSessionId: "u1", fileName: "icon.png", contentType: "image/png" },
        cover: undefined,
        screenshots: {},
      },
    };
    // The service builds metadata through the same helper used by createApp;
    // instantiate it with a minimal client seam that never runs.
    const service = createDeployAppPublishingService({
      deployClient: undefined as never,
      driveClient: undefined as never,
    });
    const metadata = service.buildMetadata(input);
    expect(metadata.sourceDirectory).toBe("/workspace/my-app");
    expect(metadata.version).toBe("1.0.0");
    expect(metadata.releaseNotes).toBe("First release");
    expect(metadata.category).toEqual({
      id: "dev-tools",
      path: [{ id: "developer", label: "开发者" }, { id: "dev-tools", label: "开发工具" }],
    });
    expect(metadata.media).toEqual(input.media);
  });

  it("drops undefined metadata keys so JSONB stays tidy", () => {
    const service = createDeployAppPublishingService({
      deployClient: undefined as never,
      driveClient: undefined as never,
    });
    const metadata = service.buildMetadata({
      sourceDirectory: "/d",
      type: staticWeb as DeployAppTypeOption,
      version: "0.1.0",
    });
    expect(metadata).not.toHaveProperty("releaseNotes");
    expect(metadata).not.toHaveProperty("category");
    expect(metadata).not.toHaveProperty("media");
    expect(metadata).not.toHaveProperty("framework");
    expect(metadata).not.toHaveProperty("buildOutputPath");
    expect(Object.keys(metadata).sort()).toEqual(["sourceDirectory", "version"]);
  });

  it("writes the v3 framework and build-output path into metadata", () => {
    const service = createDeployAppPublishingService({
      deployClient: undefined as never,
      driveClient: undefined as never,
    });
    const metadata = service.buildMetadata({
      sourceDirectory: "/workspace/apps/sdkwork-shop-h5",
      type: flutterIos as DeployAppTypeOption,
      version: "1.0.0",
      framework: "flutter",
      buildOutputPath: "build/ios/iphoneos/",
    });
    expect(metadata.framework).toBe("flutter");
    // Trailing separators are trimmed so the stored path stays canonical.
    expect(metadata.buildOutputPath).toBe("build/ios/iphoneos");
  });
});

describe("toDeployAppMediaRef", () => {
  it("maps the Drive upload result onto the persisted media reference", () => {
    const ref = toDeployAppMediaRef(
      {
        uploadSession: { id: "session-1" },
        uploadItem: { id: "item-1", spaceId: "space-1", nodeId: "node-1" },
      } as never,
      { fileName: "cover.png", contentType: "image/png", width: 1200, height: 400 },
    );
    expect(ref).toEqual({
      driveNodeId: "node-1",
      driveSpaceId: "space-1",
      uploadItemId: "item-1",
      uploadSessionId: "session-1",
      fileName: "cover.png",
      contentType: "image/png",
      width: 1200,
      height: 400,
    });
  });
});

describe("DEPLOY_APP_TYPE_OPTIONS", () => {
  it("covers the requested publish targets with platform and tech stack", () => {
    const byId = new Map(DEPLOY_APP_TYPE_OPTIONS.map((option) => [option.id, option]));
    expect(byId.get("static-web")).toMatchObject({ appKind: "STATIC_WEB", platform: "WEB" });
    expect(byId.get("wechat-mini-program")).toMatchObject({ appKind: "WECHAT_MINIPROGRAM", platform: "WECHAT" });
    expect(byId.get("flutter-ios")).toMatchObject({ appKind: "IOS_APP", platform: "IOS", techStack: "FLUTTER" });
    expect(byId.get("flutter-android")).toMatchObject({ appKind: "ANDROID_APP", platform: "ANDROID", techStack: "FLUTTER" });
    expect(byId.get("native-ios")).toMatchObject({ appKind: "IOS_APP", platform: "IOS", techStack: "NATIVE" });
    expect(byId.get("native-android")).toMatchObject({ appKind: "ANDROID_APP", platform: "ANDROID", techStack: "NATIVE" });
    expect(byId.get("harmonyos")).toMatchObject({ appKind: "HARMONYOS_APP", platform: "HARMONYOS" });
    expect(byId.get("api-service")).toMatchObject({ appKind: "API_SERVICE", platform: "API" });
  });

  it("adds the v2 h5 / pc-web / desktop targets with sdkwork surfaces", () => {
    const byId = new Map(DEPLOY_APP_TYPE_OPTIONS.map((option) => [option.id, option]));
    expect(byId.get("h5")).toMatchObject({ appKind: "SPA_WEB", platform: "WEB", surface: "h5" });
    expect(byId.get("pc-web")).toMatchObject({ appKind: "SPA_WEB", platform: "WEB", surface: "pc" });
    // Rust 契约已定义 DESKTOP_APP；生成 TS SDK 滞后，由 DeployAppKind 本地扩宽。
    expect(byId.get("desktop")).toMatchObject({ appKind: "DESKTOP_APP", surface: "desktop" });
  });

  it("adds the v3 framework-resolution rows over the full TechStack union", () => {
    const byId = new Map(DEPLOY_APP_TYPE_OPTIONS.map((option) => [option.id, option]));
    expect(byId.get("react-native-android")).toMatchObject({ appKind: "ANDROID_APP", platform: "ANDROID", techStack: "OTHER" });
    expect(byId.get("uniapp-android")).toMatchObject({ appKind: "ANDROID_APP", platform: "ANDROID", techStack: "UNI_APP" });
    expect(byId.get("uniapp-h5")).toMatchObject({ appKind: "SPA_WEB", platform: "WEB", techStack: "UNI_APP" });
    expect(byId.get("api-service-rust")).toMatchObject({ appKind: "API_SERVICE", platform: "API", techStack: "RUST" });
    expect(byId.get("api-service-node")).toMatchObject({ techStack: "NODE" });
    expect(byId.get("api-service-go")).toMatchObject({ techStack: "GO" });
    expect(byId.get("api-service-java")).toMatchObject({ techStack: "JAVA" });
    expect(byId.get("desktop-tauri")).toMatchObject({ appKind: "DESKTOP_APP", surface: "desktop" });
    expect(byId.get("uniapp-wechat-mini-program")).toMatchObject({ appKind: "WECHAT_MINIPROGRAM", techStack: "UNI_APP" });
  });
});

describe("resolveDeployAppType", () => {
  it("resolves framework-selected cards onto the concrete option rows", () => {
    expect(resolveDeployAppType("mini-program", "wechat-native")?.id).toBe("wechat-mini-program");
    expect(resolveDeployAppType("mini-program", "douyin-native")?.id).toBe("douyin-mini-program");
    expect(resolveDeployAppType("mini-program", "uniapp")?.id).toBe("uniapp-wechat-mini-program");
    expect(resolveDeployAppType("android", "flutter")?.id).toBe("flutter-android");
    expect(resolveDeployAppType("android", "react-native")?.id).toBe("react-native-android");
    expect(resolveDeployAppType("ios", "swift")?.id).toBe("native-ios");
    expect(resolveDeployAppType("ios", "uniapp")?.id).toBe("uniapp-ios");
  });

  it("falls back to the default framework and maps unsurfaced cards directly", () => {
    expect(resolveDeployAppType("android")?.id).toBe("native-android");
    expect(resolveDeployAppType("ios")?.id).toBe("native-ios");
    expect(resolveDeployAppType("h5")?.surface).toBe("h5");
    expect(resolveDeployAppType("desktop")?.id).toBe("desktop-electron");
    expect(resolveDeployAppType("desktop")?.appKind).toBe("DESKTOP_APP");
    expect(resolveDeployAppType("static-web")).toBeDefined();
    expect(resolveDeployAppType(undefined)).toBeUndefined();
    expect(resolveDeployAppType("unknown-card")).toBeUndefined();
    expect(resolveDeployAppType("android", "unknown-framework")).toBeUndefined();
  });
});

describe("frameworksOfCard (v3 framework registry)", () => {
  it("exposes industry-standard frameworks per card with build-output defaults", () => {
    expect(frameworksOfCard("h5").map((framework) => framework.id)).toEqual([
      "react", "vue", "next", "nuxt", "uniapp", "capacitor",
    ]);
    expect(frameworksOfCard("android").map((framework) => framework.id)).toEqual([
      "kotlin", "java", "flutter", "react-native", "uniapp",
    ]);
    expect(frameworksOfCard("ios").map((framework) => framework.id)).toEqual([
      "swift", "objc", "flutter", "react-native", "uniapp",
    ]);
    expect(frameworksOfCard("desktop").map((framework) => framework.id)).toEqual([
      "electron", "tauri", "qt", "flutter",
    ]);
    const nuxtH5 = frameworksOfCard("h5").find((framework) => framework.id === "nuxt");
    expect(nuxtH5?.buildOutputPath).toBe(".output/public");
  });

  it("returns an empty list for unknown cards", () => {
    expect(frameworksOfCard(undefined)).toEqual([]);
    expect(frameworksOfCard("unknown-card")).toEqual([]);
  });
});

describe("detectFrameworkId (v3.2 directory-signal detection)", () => {
  it("detects frameworks from marker directories in the surface listing", () => {
    // Flutter Android 工程：.dart_tool 标记命中 flutter。
    expect(detectFrameworkId(frameworksOfCard("android"), [".dart_tool", "android", "ios", "lib"])).toBe("flutter");
    // uni-app 产物目录：unpackage 标记命中 uniapp。
    expect(detectFrameworkId(frameworksOfCard("h5"), ["src", "unpackage"])).toBe("uniapp");
    // Nuxt：.nuxt 标记命中 nuxt（而非默认 react）。
    expect(detectFrameworkId(frameworksOfCard("h5"), [".nuxt", "app", "public"])).toBe("nuxt");
    // React Native：android + ios 双标记命中 react-native（无 .dart_tool 时）。
    expect(detectFrameworkId(frameworksOfCard("android"), ["android", "ios", "src"])).toBe("react-native");
    // Tauri 桌面：src-tauri 标记命中 tauri。
    expect(detectFrameworkId(frameworksOfCard("desktop"), ["src", "src-tauri"])).toBe("tauri");
  });

  it("prefers the earlier registry entry when multiple frameworks match", () => {
    // 注册表顺序 kotlin → java → flutter → react-native → uniapp：
    // .dart_tool 与 unpackage 同时存在时按优先级取 flutter。
    expect(detectFrameworkId(frameworksOfCard("android"), [".dart_tool", "unpackage"])).toBe("flutter");
  });

  it("returns undefined when no marker set is fully present", () => {
    // 无任何标记目录。
    expect(detectFrameworkId(frameworksOfCard("android"), ["src", "lib"])).toBeUndefined();
    // 空列举与缺失输入。
    expect(detectFrameworkId(frameworksOfCard("android"), [])).toBeUndefined();
    expect(detectFrameworkId(frameworksOfCard("android"), undefined)).toBeUndefined();
    // 未知卡片无注册表。
    expect(detectFrameworkId(frameworksOfCard("unknown-card"), ["unpackage"])).toBeUndefined();
  });
});

describe("requiredSurfaceDirectory (v4 spec path for a missing surface)", () => {
  it("composes apps/sdkwork-<code>-<suffix> per APPLICATION_SPEC", () => {
    expect(requiredSurfaceDirectory("pc", "im")).toBe("apps/sdkwork-im-pc");
    expect(requiredSurfaceDirectory("android", "im")).toBe("apps/sdkwork-im-android-mobile");
    expect(requiredSurfaceDirectory("harmony", "im")).toBe("apps/sdkwork-im-harmony-mobile");
    expect(requiredSurfaceDirectory("mini-program", "app-store")).toBe("apps/sdkwork-app-store-mini-program");
  });

  it("falls back to a placeholder code and skips root-publishing surfaces", () => {
    expect(requiredSurfaceDirectory("h5", undefined)).toBe("apps/sdkwork-<code>-h5");
    // api/static 发布仓库根目录，没有专有 apps/ 目录。
    expect(requiredSurfaceDirectory("api", "im")).toBeUndefined();
    expect(requiredSurfaceDirectory("static", "im")).toBeUndefined();
  });
});

describe("classifyAppTypeCards (v4 supported / unsupported gating)", () => {
  it("limits a sdkwork project to the surfaces under apps/", () => {
    const availability = classifyAppTypeCards(DEPLOY_APP_TYPE_CARDS, sdkworkProfile(["pc", "h5"], "im"));
    const byId = new Map(availability.map((entry) => [entry.cardId, entry]));

    expect(byId.get("h5")?.supported).toBe(true);
    expect(byId.get("pc-web")?.supported).toBe(true);
    // 项目没有这些表面 → 不支持，并给出缺失的规范目录。
    for (const cardId of ["desktop", "mini-program", "android", "ios", "harmonyos"]) {
      expect(byId.get(cardId)?.supported).toBe(false);
      expect(byId.get(cardId)?.reasonKey).toBe("typeUnsupportedRequires");
      expect(byId.get(cardId)?.requiredDirectory).toMatch(/^apps\/sdkwork-im-/);
    }
    expect(byId.get("android")?.requiredDirectory).toBe("apps/sdkwork-im-android-mobile");
    // 无表面要求的类型（发布仓库根目录）始终可选。
    expect(byId.get("api-service")?.supported).toBe(true);
    expect(byId.get("static-web")?.supported).toBe(true);
  });

  it("accepts a cross-platform surface root as proof for every target it delivers", () => {
    // -flutter-mobile 一个表面根同时交付 Android 与 iOS（APPLICATION_SPEC §2）。
    const profile = profileOfDirectories(["sdkwork-im-flutter-mobile"]);
    expect(profile.surfaces).toEqual(["android", "ios"]);
    const byId = new Map(
      classifyAppTypeCards(DEPLOY_APP_TYPE_CARDS, profile).map((entry) => [entry.cardId, entry]),
    );
    expect(byId.get("android")?.supported).toBe(true);
    expect(byId.get("ios")?.supported).toBe(true);
    expect(byId.get("pc-web")?.supported).toBe(false);
  });

  it("ignores the shared package-family root when deciding what is supported", () => {
    const profile = profileOfDirectories(["sdkwork-im-pc", "sdkwork-im-common"]);
    expect(profile.surfaces).toEqual(["pc"]);
    const byId = new Map(
      classifyAppTypeCards(DEPLOY_APP_TYPE_CARDS, profile).map((entry) => [entry.cardId, entry]),
    );
    expect(byId.get("pc-web")?.supported).toBe(true);
    expect(byId.get("h5")?.supported).toBe(false);
  });

  it("leaves every type selectable for non-sdkwork projects", () => {
    const detection = detectSdkworkProject({
      rootPath: "E:\\ws\\my-vite-app",
      childDirectories: ["src", "public"],
    });
    const profile = projectProfile(detection, "E:\\ws\\my-vite-app");
    expect(profile.sdkwork).toBe(false);
    const availability = classifyAppTypeCards(DEPLOY_APP_TYPE_CARDS, profile);
    expect(availability.every((entry) => entry.supported)).toBe(true);
  });

  it("stays permissive when the apps/ listing is unavailable", () => {
    // 用户走进表面根内部：目录名证明是 sdkwork 项目，但读不到 apps/ 清单 ——
    // 不得据此把其他类型误判为不支持。
    const profile = projectProfile(undefined, "E:\\ws\\sdkwork-im\\apps\\sdkwork-im-h5");
    expect(profile.sdkwork).toBe(true);
    expect(profile.surfaces).toEqual([]);
    const availability = classifyAppTypeCards(DEPLOY_APP_TYPE_CARDS, profile);
    expect(availability.every((entry) => entry.supported)).toBe(true);
  });
});

describe("classifyFrameworks (v4 project-architecture alignment)", () => {
  const conflictsOf = (cardId: string, children: readonly string[] | undefined): readonly string[] =>
    classifyFrameworks(frameworksOfCard(cardId), children)
      .filter((entry) => entry.conflicting)
      .map((entry) => entry.frameworkId);

  it("flags frameworks whose markers are provably absent while another architecture matched", () => {
    // uni-app 工程（unpackage 决定性命中）→ Next/Nuxt/Capacitor 与项目架构不符。
    expect(conflictsOf("h5", ["src", "unpackage"])).toEqual(["next", "nuxt", "capacitor"]);
    // Flutter 移动工程 → 未使用跨端/RN 路径的框架与项目架构不符。
    expect(conflictsOf("android", [".dart_tool", "lib"])).toEqual(["react-native", "uniapp"]);
  });

  it("keeps marker-less fallback frameworks selectable on purpose", () => {
    // React / Vue（无标识目录）不会被判冲突，用户可以自行选择。
    expect(conflictsOf("h5", ["src", "unpackage"])).not.toContain("react");
    expect(conflictsOf("h5", ["src", "unpackage"])).not.toContain("vue");
    // Kotlin / Java 是 Android 的兜底默认项，不参与冲突判定。
    expect(conflictsOf("android", ["android", "ios", "src"])).toEqual(["flutter", "uniapp"]);
  });

  it("finds no conflict when the directory yields no decisive architecture", () => {
    expect(conflictsOf("h5", ["src", "public", "node_modules"])).toEqual([]);
    expect(conflictsOf("h5", [])).toEqual([]);
    expect(conflictsOf("h5", undefined)).toEqual([]);
  });

  it("marks the decisive framework as detected exactly once", () => {
    const detected = classifyFrameworks(frameworksOfCard("desktop"), ["src", "src-tauri"])
      .filter((entry) => entry.detected)
      .map((entry) => entry.frameworkId);
    expect(detected).toEqual(["tauri"]);
  });
});

/**
 * 验收用例：真实工程 sdkwork-im 的目录（E:\sdkwork-space\sdkwork-im，2026-09 实际
 * 结构）—— 根含 apps/deployments/etc/specs/.sdkwork，apps/ 下只有
 * `-pc`、`-h5`、`-flutter-mobile` 三个表面根，其中 flutter-mobile 含 `.dart_tool`。
 *
 * 期望：对话框只放开 PC 网页 / H5 / Android / iOS（+ 发布仓库根目录的 API 服务与
 * 静态资源），PC 桌面 / 小程序 / 鸿蒙置灰并标出缺失目录；Android 类型下 Flutter
 * 被判定为项目架构（uni-app 置灰）。
 */
describe("acceptance: real sdkwork-im layout", () => {
  const listing = {
    rootPath: "E:\\sdkwork-space\\sdkwork-im",
    childDirectories: [
      "adapters", "apis", "apps", "artifacts", "bin", "config", "crates", "data", "database",
      "deployments", "docs", "etc", "examples", "generated", "jobs", "plugins", "scripts",
      "sdks", "services", "specs", "target", "tests", "tools", "vendor", ".sdkwork",
    ],
    appsChildDirectories: ["sdkwork-im-flutter-mobile", "sdkwork-im-h5", "sdkwork-im-pc"],
    surfaceChildDirectories: {
      "sdkwork-im-flutter-mobile": [
        "android", "build", "config", "env", "etc", "ios", "lib", "linux", "macos",
        "packages", "scripts", "specs", "test", "web", "windows", ".dart_tool",
      ],
      "sdkwork-im-h5": ["bin", "config", "dist", "docs", "etc", "packages", "public", "scripts", "src", "tests"],
      "sdkwork-im-pc": ["dist", "docs", "e2e", "etc", "packages", "public", "scripts", "src", "test-results"],
    },
  };

  it("recognizes the repository as a conformant sdkwork project", () => {
    const detection = detectSdkworkProject(listing);
    const profile = projectProfile(detection, listing.rootPath);
    expect(profile.sdkwork).toBe(true);
    expect(profile.applicationCode).toBe("im");
    expect(profile.surfaces).toEqual(["pc", "h5", "android", "ios"]);
  });

  it("exposes exactly the supported application types and disables the rest", () => {
    const profile = projectProfile(detectSdkworkProject(listing), listing.rootPath);
    const byId = new Map(
      classifyAppTypeCards(DEPLOY_APP_TYPE_CARDS, profile).map((entry) => [entry.cardId, entry]),
    );

    for (const cardId of ["pc-web", "h5", "android", "ios", "api-service", "static-web"]) {
      expect(byId.get(cardId)?.supported, `${cardId} should be selectable`).toBe(true);
    }
    for (const cardId of ["desktop", "mini-program", "harmonyos"]) {
      expect(byId.get(cardId)?.supported, `${cardId} should be disabled`).toBe(false);
    }
    expect(byId.get("desktop")?.requiredDirectory).toBe("apps/sdkwork-im-desktop");
    expect(byId.get("mini-program")?.requiredDirectory).toBe("apps/sdkwork-im-mini-program");
    expect(byId.get("harmonyos")?.requiredDirectory).toBe("apps/sdkwork-im-harmony-mobile");
  });

  it("keeps the Android choice aligned with the project's Flutter architecture", () => {
    const flutterMobile = listing.surfaceChildDirectories["sdkwork-im-flutter-mobile"];
    const availability = classifyFrameworks(frameworksOfCard("android"), flutterMobile);
    const byId = new Map(availability.map((entry) => [entry.frameworkId, entry]));

    // .dart_tool → Flutter 即项目架构，自动选中。
    expect(byId.get("flutter")?.detected).toBe(true);
    expect(byId.get("flutter")?.conflicting).toBe(false);
    // 项目未使用 uni-app → 该选项与架构不符。
    expect(byId.get("uniapp")?.conflicting).toBe(true);
    // React Native 的 android/ 目录确实存在 → 不判冲突（只做可证伪的排除）。
    expect(byId.get("react-native")?.conflicting).toBe(false);
  });

  it("prefers the dedicated h5/pc roots over the cross-platform mobile root", () => {
    const detection = detectSdkworkProject(listing);
    expect(resolveSourceDirectory(detection, "h5", listing.rootPath))
      .toBe("E:\\sdkwork-space\\sdkwork-im\\apps\\sdkwork-im-h5");
    expect(resolveSourceDirectory(detection, "pc", listing.rootPath))
      .toBe("E:\\sdkwork-space\\sdkwork-im\\apps\\sdkwork-im-pc");
    // Android / iOS 没有专有根 → 落到 flutter-mobile。
    expect(resolveSourceDirectory(detection, "android", listing.rootPath))
      .toBe("E:\\sdkwork-space\\sdkwork-im\\apps\\sdkwork-im-flutter-mobile");
    expect(resolveSourceDirectory(detection, "ios", listing.rootPath))
      .toBe("E:\\sdkwork-space\\sdkwork-im\\apps\\sdkwork-im-flutter-mobile");
  });
});
