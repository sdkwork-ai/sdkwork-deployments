/**
 * Unit tests for the sdkwork-specs project auto-detection used by the
 * create-deploy-app dialog: the reusable {@link SdkworkProject} class, the
 * dialog-side surface mapping, deployable-root conformance markers, and the
 * canonical environment helpers. Pure functions only — no clients, no IO.
 */
import { describe, expect, it } from "vitest";
import {
  APP_SURFACE_DIRECTORY_CAPABILITIES,
  APP_SURFACE_DIRECTORY_SUFFIX,
  browserDistOutputPath,
  buildOutputExists,
  canonicalEnvironment,
  deriveSurfaceDirectory,
  detectBuildOutputCandidates,
  detectedSurfaceIds,
  DEPLOY_DEPLOYMENT_MODES,
  DEPLOY_ENVIRONMENT_IDS,
  deployProfileId,
  detectSdkworkProject,
  findDetectedSurface,
  joinPath,
  projectProfile,
  resolveSourceDirectory,
  surfacesOfDirectoryName,
  type DeployProjectInspection,
} from "../src/service/project-detection.ts";
import {
  SDKWORK_SURFACE_ARCHITECTURES,
  SdkworkProject,
} from "../src/service/sdkwork-project.ts";

const conformantInspection: DeployProjectInspection = {
  rootPath: "/workspace/store",
  childDirectories: ["apps", "deployments", "etc", "specs", ".sdkwork", "crates"],
  appsChildDirectories: [
    "sdkwork-store-pc",
    "sdkwork-store-h5",
    "sdkwork-store-mini-program",
    "sdkwork-store-android-mobile",
    "sdkwork-store-ios-mobile",
    "not-sdkwork",
  ],
};

describe("surfacesOfDirectoryName", () => {
  it("maps sdkwork surface directories onto dialog surface ids", () => {
    expect(surfacesOfDirectoryName("sdkwork-store-pc"))
      .toEqual({ surfaces: ["pc"], directorySuffix: "pc", applicationCode: "store" });
    expect(surfacesOfDirectoryName("sdkwork-store-h5"))
      .toEqual({ surfaces: ["h5"], directorySuffix: "h5", applicationCode: "store" });
    expect(surfacesOfDirectoryName("sdkwork-store-mini-program")).toEqual({
      surfaces: ["mini-program"],
      directorySuffix: "mini-program",
      applicationCode: "store",
    });
    expect(surfacesOfDirectoryName("sdkwork-store-android-mobile")).toEqual({
      surfaces: ["android"],
      directorySuffix: "android-mobile",
      applicationCode: "store",
    });
    expect(surfacesOfDirectoryName("sdkwork-store-ios-mobile"))
      .toEqual({ surfaces: ["ios"], directorySuffix: "ios-mobile", applicationCode: "store" });
    expect(surfacesOfDirectoryName("sdkwork-app-store-desktop")).toEqual({
      surfaces: ["desktop"],
      directorySuffix: "desktop",
      applicationCode: "app-store",
    });
    expect(surfacesOfDirectoryName("sdkwork-app-store-harmony-mobile")).toEqual({
      surfaces: ["harmony"],
      directorySuffix: "harmony-mobile",
      applicationCode: "app-store",
    });
  });

  it("keeps multi-segment application codes via a greedy code capture", () => {
    expect(surfacesOfDirectoryName("sdkwork-app-store-pc")?.applicationCode).toBe("app-store");
  });

  it("expands cross-platform roots onto every publish target they deliver", () => {
    // APPLICATION_SPEC §2 的完整架构表：一个表面根可交付多个发布目标。
    expect(surfacesOfDirectoryName("sdkwork-im-flutter-mobile")?.surfaces).toEqual(["android", "ios"]);
    expect(surfacesOfDirectoryName("sdkwork-im-uniapp")?.surfaces)
      .toEqual(["h5", "mini-program", "android", "ios", "harmony"]);
    expect(surfacesOfDirectoryName("sdkwork-store-static-web")?.surfaces).toEqual(["static"]);
    expect(surfacesOfDirectoryName("sdkwork-game-unity")?.surfaces).toEqual(["android", "ios"]);
    expect(surfacesOfDirectoryName("sdkwork-store-pad")?.surfaces).toEqual(["android", "ios", "harmony"]);
  });

  it("never treats the shared package-family root as a publishable surface", () => {
    expect(surfacesOfDirectoryName("sdkwork-im-common")).toBeUndefined();
  });

  it("rejects non-surface and malformed names", () => {
    expect(surfacesOfDirectoryName("sdkwork-store")).toBeUndefined();
    expect(surfacesOfDirectoryName("not-sdkwork")).toBeUndefined();
    expect(surfacesOfDirectoryName("sdkwork-store-web")).toBeUndefined();
    expect(surfacesOfDirectoryName("")).toBeUndefined();
  });

  it("covers every canonical architecture suffix in the dialog table", () => {
    for (const architecture of SDKWORK_SURFACE_ARCHITECTURES) {
      expect(APP_SURFACE_DIRECTORY_CAPABILITIES[architecture]?.length ?? 0).toBeGreaterThan(0);
    }
  });
});

describe("SdkworkProject (reusable spec detector)", () => {
  it("answers the path-only question without any directory listing", () => {
    expect(SdkworkProject.isSdkworkDirectory("E:\\ws\\sdkwork-im")).toBe(true);
    expect(SdkworkProject.isSdkworkDirectory("/ws/sdkwork-im/apps/sdkwork-im-pc")).toBe(true);
    expect(SdkworkProject.isSdkworkDirectory("E:\\ws\\my-vite-app")).toBe(false);
    expect(SdkworkProject.isSdkworkDirectory(undefined)).toBe(false);
    expect(SdkworkProject.isSdkworkDirectory("E:\\ws\\sdkwork-im\\apps")).toBe(false);
  });

  it("derives the application code from either a repo root or a surface root", () => {
    expect(SdkworkProject.applicationCodeOfDirectory("E:\\ws\\sdkwork-im")).toBe("im");
    expect(SdkworkProject.applicationCodeOfDirectory("/ws/sdkwork-im/apps/sdkwork-im-flutter-mobile")).toBe("im");
    expect(SdkworkProject.applicationCodeOfDirectory("/ws/my-app")).toBeUndefined();
  });

  it("parses the surface grammar and rejects the shared package family", () => {
    expect(SdkworkProject.parseSurfaceDirectoryName("sdkwork-im-flutter-mobile")).toEqual({
      directory: "sdkwork-im-flutter-mobile",
      applicationCode: "im",
      architecture: "flutter-mobile",
    });
    expect(SdkworkProject.parseSurfaceDirectoryName("sdkwork-im-common")).toBeUndefined();
    expect(SdkworkProject.parseSurfaceDirectoryName("sdkwork-im")).toBeUndefined();
  });

  it("models an inspected repository into architecture queries", () => {
    const project = SdkworkProject.inspect({
      rootPath: "E:\\ws\\sdkwork-im",
      childDirectories: ["apps", "deployments", "etc", "specs", ".sdkwork", "crates"],
      appsChildDirectories: ["sdkwork-im-pc", "sdkwork-im-h5", "sdkwork-im-flutter-mobile", "sdkwork-im-common"],
      surfaceChildDirectories: { "sdkwork-im-h5": ["src", "dist"] },
    });

    expect(project.isSdkworkProject).toBe(true);
    expect(project.conformance).toBe("conformant");
    expect(project.hasSurfaceListing).toBe(true);
    expect(project.applicationCode).toBe("im");
    // 架构后缀是规范词汇（surface 根的目录后缀），不是对话框卡片 id。
    expect(project.architectures).toEqual(["pc", "h5", "flutter-mobile"]);
    expect(project.has("pc")).toBe(true);
    expect(project.has("harmony-mobile")).toBe(false);
    // 一个表面根对应一个架构；「一个架构可以由哪些根交付」是对话框层的映射
    // （APP_SURFACE_DIRECTORY_CAPABILITIES），通用类不掺入该词汇。
    expect(project.find("flutter-mobile")?.directory).toBe("sdkwork-im-flutter-mobile");
    expect(project.find("android-mobile")).toBeUndefined();
    expect(project.childDirectoriesOf("h5")).toEqual(["src", "dist"]);
    expect(project.childDirectoriesOf("pc")).toBeUndefined();
    expect(project.sourceDirectoryFor("h5")).toBe("E:\\ws\\sdkwork-im\\apps\\sdkwork-im-h5");
    // 无表面架构（API 服务 / 静态资源）发布仓库根目录，框架判定回退根目录列举。
    expect(project.sourceDirectoryFor(undefined)).toBe("E:\\ws\\sdkwork-im");
    expect(project.childDirectoriesOf(undefined)).toContain("crates");
  });

  it("stays honest when the host cannot list apps/", () => {
    const project = SdkworkProject.inspect({
      rootPath: "E:\\ws\\sdkwork-im\\apps\\sdkwork-im-h5",
      childDirectories: ["src", "public", "node_modules"],
    });
    // 目录名证明是 sdkwork 项目，但没有表面清单 —— 上层不得据此判「不支持」。
    expect(project.isSdkworkProject).toBe(true);
    expect(project.hasSurfaceListing).toBe(false);
    expect(project.surfaces).toEqual([]);
    expect(project.applicationCode).toBe("im");
    expect(project.conformance).toBe("unknown");
  });

  it("does not mistake a plain project for a sdkwork repository", () => {
    const project = SdkworkProject.inspect({
      rootPath: "E:\\ws\\my-vite-app",
      childDirectories: ["src", "public", "node_modules"],
    });
    expect(project.isSdkworkProject).toBe(false);
    expect(project.applicationCode).toBeUndefined();
  });

  it("degrades to a directory-name-only model without any input", () => {
    const project = SdkworkProject.ofDirectory("/ws/sdkwork-store");
    expect(project.isSdkworkProject).toBe(true);
    expect(project.applicationCode).toBe("store");
    expect(project.surfaces).toEqual([]);
  });

  it("resolves the owning repository root from inside an apps/ surface root", () => {
    // 站在表面根内部时，宿主列举看不到同级表面 —— 需要回到仓库根再列举一次。
    expect(SdkworkProject.repositoryRootOfDirectory("E:\\ws\\sdkwork-im\\apps\\sdkwork-im-h5"))
      .toBe("E:\\ws\\sdkwork-im");
    expect(SdkworkProject.repositoryRootOfDirectory("/ws/sdkwork-im/apps/sdkwork-im-flutter-mobile"))
      .toBe("/ws/sdkwork-im");
    expect(SdkworkProject.repositoryRootOfDirectory("sdkwork-im/apps/sdkwork-im-pc")).toBe("sdkwork-im");
  });

  it("reports no repository root for non-conforming paths", () => {
    // 已在仓库根：没有可回退的上级。
    expect(SdkworkProject.repositoryRootOfDirectory("E:\\ws\\sdkwork-im")).toBeUndefined();
    // 表面目录但父级不是 apps/（不符规范布局）→ 不猜。
    expect(SdkworkProject.repositoryRootOfDirectory("E:\\other\\sdkwork-im-h5")).toBeUndefined();
    expect(SdkworkProject.repositoryRootOfDirectory("E:\\other\\apps\\my-app")).toBeUndefined();
    expect(SdkworkProject.repositoryRootOfDirectory(undefined)).toBeUndefined();
    expect(SdkworkProject.repositoryRootOfDirectory("")).toBeUndefined();
  });
});

describe("projectProfile (dialog gating input)", () => {
  it("gates types only for a sdkwork project with a readable apps/ listing", () => {
    const detection = detectSdkworkProject(conformantInspection);
    const profile = projectProfile(detection, conformantInspection.rootPath);
    expect(profile.sdkwork).toBe(true);
    expect(profile.surfaces).toEqual(["pc", "h5", "mini-program", "android", "ios"]);
    expect(profile.applicationCode).toBe("store");
  });

  it("recognizes a sdkwork project even when the listing is unavailable", () => {
    const profile = projectProfile(undefined, "E:\\ws\\sdkwork-im\\apps\\sdkwork-im-h5");
    expect(profile.sdkwork).toBe(true);
    expect(profile.surfaces).toEqual([]);
    expect(profile.applicationCode).toBe("im");
  });

  it("reports a generic project so every application type stays selectable", () => {
    const detection = detectSdkworkProject({
      rootPath: "/workspace/random",
      childDirectories: ["src", "public"],
    });
    const profile = projectProfile(detection, "/workspace/random");
    expect(profile.sdkwork).toBe(false);
    expect(profile.surfaces).toEqual([]);
  });
});

describe("detectSdkworkProject", () => {
  it("reports a conformant root with every spec marker present", () => {
    const detection = detectSdkworkProject(conformantInspection);
    expect(detection.conformance).toBe("conformant");
    expect(detection.missingMarkers).toEqual([]);
    expect(detection.applicationCode).toBe("store");
    expect(detection.project.isSdkworkProject).toBe(true);
  });

  it("detects surfaces in canonical order with joined paths", () => {
    const detection = detectSdkworkProject(conformantInspection);
    expect(detection.surfaces.map((surface) => surface.directory)).toEqual([
      "sdkwork-store-pc",
      "sdkwork-store-h5",
      "sdkwork-store-mini-program",
      "sdkwork-store-android-mobile",
      "sdkwork-store-ios-mobile",
    ]);
    expect(detectedSurfaceIds(detection)).toEqual(["pc", "h5", "mini-program", "android", "ios"]);
    expect(detection.surfaces[0]?.path).toBe("/workspace/store/apps/sdkwork-store-pc");
  });

  it("carries the inspected root children for root-publishing types", () => {
    const detection = detectSdkworkProject(conformantInspection);
    expect(detection.rootChildDirectories).toContain("crates");
  });

  it("degrades to partial when only some markers exist", () => {
    const detection = detectSdkworkProject({
      rootPath: "/workspace/store",
      childDirectories: ["apps", "etc"],
    });
    expect(detection.conformance).toBe("partial");
    expect(detection.presentMarkers).toEqual(["apps", "etc"]);
    expect(detection.surfaces).toEqual([]);
    expect(detection.applicationCode).toBeUndefined();
  });

  it("reports unknown for directories without sdkwork markers", () => {
    const detection = detectSdkworkProject({
      rootPath: "/workspace/random",
      childDirectories: ["src", "public"],
      appsChildDirectories: undefined,
    });
    expect(detection.conformance).toBe("unknown");
    expect(detection.missingMarkers).toEqual(["apps", "deployments", "etc", "specs", ".sdkwork"]);
  });

  it("keeps the application code undefined when surfaces disagree", () => {
    const detection = detectSdkworkProject({
      rootPath: "/workspace/multi",
      childDirectories: ["apps", "etc", "specs", "deployments", ".sdkwork"],
      appsChildDirectories: ["sdkwork-one-pc", "sdkwork-two-h5"],
    });
    expect(detection.applicationCode).toBeUndefined();
    expect(detection.surfaces).toHaveLength(2);
  });
});

describe("resolveSourceDirectory", () => {
  it("prefers the detected surface root for the selected surface", () => {
    const detection = detectSdkworkProject(conformantInspection);
    expect(resolveSourceDirectory(detection, "h5", conformantInspection.rootPath)).toBe(
      "/workspace/store/apps/sdkwork-store-h5",
    );
  });

  it("falls back to the root when the surface is unsurfaced or undetected", () => {
    const detection = detectSdkworkProject(conformantInspection);
    expect(resolveSourceDirectory(detection, "api", conformantInspection.rootPath)).toBe("/workspace/store");
    expect(resolveSourceDirectory(detection, "harmony", conformantInspection.rootPath)).toBe("/workspace/store");
    expect(resolveSourceDirectory(detection, undefined, conformantInspection.rootPath)).toBe("/workspace/store");
  });

  it("prefers a dedicated apps/ root over a cross-platform root", () => {
    const detection = detectSdkworkProject({
      rootPath: "/workspace/im",
      childDirectories: ["apps", "etc", "specs"],
      appsChildDirectories: ["sdkwork-im-flutter-mobile", "sdkwork-im-android-mobile"],
    });
    expect(findDetectedSurface(detection, "android")?.directory).toBe("sdkwork-im-android-mobile");
    expect(resolveSourceDirectory(detection, "android", "/workspace/im"))
      .toBe("/workspace/im/apps/sdkwork-im-android-mobile");
    // iOS 只有跨端根可承载 → 回退到 flutter-mobile。
    expect(resolveSourceDirectory(detection, "ios", "/workspace/im"))
      .toBe("/workspace/im/apps/sdkwork-im-flutter-mobile");
  });
});

describe("environment helpers", () => {
  it("keeps the canonical environment set ordered", () => {
    expect(DEPLOY_ENVIRONMENT_IDS).toEqual(["development", "test", "staging", "demo", "production"]);
  });

  it("composes canonical profile ids", () => {
    expect(deployProfileId("standalone", "production")).toBe("standalone.production");
    expect(deployProfileId("cloud", "development")).toBe("cloud.development");
  });

  it("normalizes legacy command aliases", () => {
    expect(canonicalEnvironment("dev")).toBe("development");
    expect(canonicalEnvironment("PROD")).toBe("production");
    expect(canonicalEnvironment("staging")).toBe("staging");
  });
});

describe("surface suffix table", () => {
  it("matches the sdkwork-specs apps/ directory grammar", () => {
    expect(APP_SURFACE_DIRECTORY_SUFFIX.android).toBe("android-mobile");
    expect(APP_SURFACE_DIRECTORY_SUFFIX.ios).toBe("ios-mobile");
    expect(APP_SURFACE_DIRECTORY_SUFFIX["mini-program"]).toBe("mini-program");
    expect(APP_SURFACE_DIRECTORY_SUFFIX.api).toBe("");
  });
});

describe("joinPath", () => {
  it("joins segments without duplicating separators", () => {
    expect(joinPath("/workspace/app/", "apps", "sdkwork-app-pc")).toBe("/workspace/app/apps/sdkwork-app-pc");
    expect(joinPath("C:\\workspace\\app", "apps")).toBe("C:\\workspace\\app/apps");
    expect(joinPath("/root", "")).toBe("/root");
  });
});

describe("v3 build-output detection (dual paths)", () => {
  const inspection: DeployProjectInspection = {
    rootPath: "/workspace/store",
    childDirectories: ["apps", "deployments", "etc", "specs", ".sdkwork"],
    appsChildDirectories: ["sdkwork-store-h5"],
    surfaceChildDirectories: {
      "sdkwork-store-h5": ["src", "dist", "public", "node_modules"],
    },
  };

  it("carries surface child directories onto the detected surfaces", () => {
    const detection = detectSdkworkProject(inspection);
    expect(detection.surfaces[0]?.childDirectories).toEqual(["src", "dist", "public", "node_modules"]);
  });

  it("omits child directories when the host did not list them", () => {
    const detection = detectSdkworkProject({
      rootPath: "/workspace/store",
      childDirectories: ["apps"],
      appsChildDirectories: ["sdkwork-store-h5"],
    });
    expect(detection.surfaces[0]?.childDirectories).toBeUndefined();
  });

  it("validates a relative build-output path against the surface listing", () => {
    expect(buildOutputExists("dist", ["src", "dist"])).toBe(true);
    expect(buildOutputExists("./dist", ["src", "dist"])).toBe(true);
    expect(buildOutputExists(".output/public", ["src", ".output"])).toBe(true);
    expect(buildOutputExists("build", ["src", "dist"])).toBe(false);
    expect(buildOutputExists("../outside", ["src"])).toBe(false);
    expect(buildOutputExists("C:\\abs", ["src"])).toBe(false);
  });

  it("treats the root itself and unknown listings as existing/unknowable", () => {
    expect(buildOutputExists(".", ["src"])).toBe(true);
    expect(buildOutputExists("", ["src"])).toBe(true);
    expect(buildOutputExists("dist", undefined)).toBeUndefined();
  });

  it("collects generic build-output candidates from the surface listing", () => {
    expect(detectBuildOutputCandidates(["src", "dist", "public", "node_modules"])).toEqual(["dist", "public"]);
    expect(detectBuildOutputCandidates(["src"])).toEqual([]);
    expect(detectBuildOutputCandidates(undefined)).toEqual([]);
  });
});

describe("deriveSurfaceDirectory (v3.3 spec path derivation)", () => {
  const winRepo = "E:\\sdkwork-space\\sdkwork-cloudrouter";
  const winApps = "E:\\sdkwork-space\\sdkwork-cloudrouter\\apps";

  it("derives apps/<surface> roots from a sdkwork repo root, preserving separators", () => {
    expect(deriveSurfaceDirectory(winRepo, "h5")).toBe(`${winApps}\\sdkwork-cloudrouter-h5`);
    expect(deriveSurfaceDirectory("/workspace/sdkwork-store", "h5"))
      .toBe("/workspace/sdkwork-store/apps/sdkwork-store-h5");
    // 不同应用类型映射到各自规范后缀（APPLICATION_SPEC §2）。
    expect(deriveSurfaceDirectory("E:\\repo\\sdkwork-cloudrouter", "pc"))
      .toBe("E:\\repo\\sdkwork-cloudrouter\\apps\\sdkwork-cloudrouter-pc");
    expect(deriveSurfaceDirectory("E:\\repo\\sdkwork-cloudrouter", "android"))
      .toBe("E:\\repo\\sdkwork-cloudrouter\\apps\\sdkwork-cloudrouter-android-mobile");
    expect(deriveSurfaceDirectory("E:\\repo\\sdkwork-cloudrouter", "ios"))
      .toBe("E:\\repo\\sdkwork-cloudrouter\\apps\\sdkwork-cloudrouter-ios-mobile");
    expect(deriveSurfaceDirectory("E:\\repo\\sdkwork-cloudrouter", "mini-program"))
      .toBe("E:\\repo\\sdkwork-cloudrouter\\apps\\sdkwork-cloudrouter-mini-program");
    expect(deriveSurfaceDirectory("E:\\repo\\sdkwork-cloudrouter", "harmony"))
      .toBe("E:\\repo\\sdkwork-cloudrouter\\apps\\sdkwork-cloudrouter-harmony-mobile");
  });

  it("keeps an already-correct surface root and switches between sibling surfaces", () => {
    // 已是目标表面根 → 无需完善。
    expect(deriveSurfaceDirectory(`${winApps}\\sdkwork-cloudrouter-h5`, "h5")).toBeUndefined();
    // 站在其他表面根时切换类型 → 推导同级表面目录。
    expect(deriveSurfaceDirectory(`${winApps}\\sdkwork-cloudrouter-h5`, "pc"))
      .toBe(`${winApps}\\sdkwork-cloudrouter-pc`);
    // 表面目录但父级不是 apps/（非规范布局）→ 不猜。
    expect(deriveSurfaceDirectory("E:\\other\\sdkwork-cloudrouter-h5", "pc")).toBeUndefined();
  });

  it("returns undefined for non-repo directories and root-publishing surfaces", () => {
    expect(deriveSurfaceDirectory("E:\\tools", "h5")).toBeUndefined();
    expect(deriveSurfaceDirectory("", "h5")).toBeUndefined();
    // api/static 发布仓库根本身，无 apps/ 表面目录。
    expect(deriveSurfaceDirectory(winRepo, "api")).toBeUndefined();
    expect(deriveSurfaceDirectory(winRepo, "static")).toBeUndefined();
  });
});

describe("browserDistOutputPath (v3.4 environment-aware dist layout)", () => {
  it("composes dist/<deploymentProfile>/<envAlias> per FRONTEND_CODE_SPEC §7", () => {
    expect(browserDistOutputPath("standalone", "development")).toBe("dist/standalone/dev");
    expect(browserDistOutputPath("standalone", "test")).toBe("dist/standalone/test");
    expect(browserDistOutputPath("standalone", "staging")).toBe("dist/standalone/staging");
    expect(browserDistOutputPath("standalone", "production")).toBe("dist/standalone/prod");
    expect(browserDistOutputPath("cloud", "production")).toBe("dist/cloud/prod");
    expect(browserDistOutputPath("cloud", "development")).toBe("dist/cloud/dev");
  });

  it("never returns a bare dist/ for any mode × environment combination", () => {
    for (const mode of DEPLOY_DEPLOYMENT_MODES) {
      for (const environment of DEPLOY_ENVIRONMENT_IDS) {
        const output = browserDistOutputPath(mode, environment);
        expect(output.startsWith(`dist/${mode}/`)).toBe(true);
        expect(output).not.toBe("dist");
      }
    }
  });

  it("builds demo from the staging subtree (spec alias table lacks demo)", () => {
    expect(browserDistOutputPath("standalone", "demo")).toBe("dist/standalone/staging");
    expect(browserDistOutputPath("cloud", "demo")).toBe("dist/cloud/staging");
  });
});
