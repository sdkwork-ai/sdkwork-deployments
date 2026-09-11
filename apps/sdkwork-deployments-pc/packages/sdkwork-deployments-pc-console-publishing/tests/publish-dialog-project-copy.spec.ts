/**
 * 发布对话框「项目路径栏」文案策略回归测试。
 *
 * 回归背景：路径栏曾把「规范判定」同时渲染两次 —— meta 行的徽标与下方 rule
 * 行各自输出同一句文案，非 sdkwork 目录上表现为「非 sdkwork 规范项目 —— 全部
 * 应用类型均可选择。」上下重复出现两遍。修复后：
 *
 *   - 非 sdkwork 目录不输出任何规范文案（只有路径 + 更换 + 检测中的反馈）；
 *   - sdkwork 项目的规范约束说明在整栏里只出现一次；
 *   - 「非 sdkwork 规范项目」这一词组连同字典键一起删除，不再存在。
 *
 * 断言基于 react-dom/server 的静态标记 + 真实检测函数（不 mock 目录列举），
 * 因此同时覆盖 projectProfile 的判定边界。
 */
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { DeployProjectDirectoryFields } from "../src/components/DeployProjectDirectoryFields.tsx";
import { DeployProjectPathBar } from "../src/components/DeployProjectPathBar.tsx";
import { publishingTranslator } from "../src/i18n.ts";
import {
  detectSdkworkProject,
  projectProfile,
  type DeployProjectInspection,
} from "../src/service/project-detection.ts";

const zh = publishingTranslator("zh-CN");

/** 渲染路径栏，全部输入走真实检测链路。 */
function renderPathBar(directory: string, inspection?: DeployProjectInspection): string {
  const detection = inspection === undefined ? undefined : detectSdkworkProject(inspection);
  return renderToStaticMarkup(createElement(DeployProjectPathBar, {
    directory,
    inspecting: false,
    detection,
    profile: projectProfile(detection, directory),
    t: zh,
    onChangeDirectory: () => {},
  }));
}

/** 一个普通静态目录（非 sdkwork 规范项目），与用户上报的 D:\\temp\\static-html 同形。 */
const PLAIN_DIRECTORY = "D:/temp/static-html";
const PLAIN_INSPECTION: DeployProjectInspection = {
  rootPath: PLAIN_DIRECTORY,
  childDirectories: ["index.html", "assets", "css"],
  appsChildDirectories: [],
  surfaceChildDirectories: {},
};

/** sdkwork 规范项目：apps/ 下有可发布的 pc 表面。 */
const SDKWORK_ROOT = "E:/work/sdkwork-demo";
const SDKWORK_INSPECTION: DeployProjectInspection = {
  rootPath: SDKWORK_ROOT,
  childDirectories: ["apps", "deployments", "etc", "specs", ".sdkwork"],
  appsChildDirectories: ["sdkwork-demo-pc"],
  surfaceChildDirectories: { "sdkwork-demo-pc": ["dist", "src"] },
};

/** 出现次数（用于「同一句话不得重复渲染」的断言）。 */
function occurrences(haystack: string, needle: string): number {
  return haystack.split(needle).length - 1;
}

describe("DeployProjectPathBar — 非 sdkwork 目录不输出规范文案", () => {
  it("只显示路径，不再出现任何规范判定文案", () => {
    const markup = renderPathBar(PLAIN_DIRECTORY, PLAIN_INSPECTION);

    expect(markup).toContain(PLAIN_DIRECTORY);
    expect(markup).toContain(zh("changeDirectory"));

    for (const removed of ["非 sdkwork", "规范项目", "规范目录", "全部应用类型", "未识别为", "应用表面"]) {
      expect(markup).not.toContain(removed);
    }
  });

  it("检测结果缺失时同样保持沉默（不退回 generic 文案）", () => {
    const markup = renderPathBar(PLAIN_DIRECTORY);

    expect(markup).toContain(PLAIN_DIRECTORY);
    for (const removed of ["非 sdkwork", "规范项目", "全部应用类型"]) {
      expect(markup).not.toContain(removed);
    }
  });

  it("未选中目录时只显示占位，不附带规范说明", () => {
    const markup = renderPathBar("");

    expect(markup).toContain(zh("noDirectory"));
    expect(markup).not.toContain("规范项目");
  });
});

describe("DeployProjectPathBar — sdkwork 项目的约束说明只出现一次", () => {
  it("apps/ 表面清单可读：输出一次 gated 说明，不重复", () => {
    const markup = renderPathBar(SDKWORK_ROOT, SDKWORK_INSPECTION);
    const gate = zh("projectKindSdkworkGate");

    expect(occurrences(markup, gate)).toBe(1);
    expect(markup).not.toContain(zh("projectKindSdkworkUnlisted"));
  });

  it("未读到 apps/ 表面清单：输出一次 unlisted 说明，不重复", () => {
    const markup = renderPathBar(SDKWORK_ROOT, {
      rootPath: SDKWORK_ROOT,
      childDirectories: ["specs", "docs"],
      appsChildDirectories: [],
      surfaceChildDirectories: {},
    });
    const unlisted = zh("projectKindSdkworkUnlisted");

    expect(occurrences(markup, unlisted)).toBe(1);
    expect(markup).not.toContain(zh("projectKindSdkworkGate"));
    // 符合度未知时不出徽标：否则「未识别为 sdkwork 规范目录」会与上面那句
    // 「sdkwork 项目…」自相矛盾。
    expect(markup).not.toContain(zh("detectionUnknown"));
  });
});

/** 第 2 步（目录 / 产物 / 框架）里的规范检测面板。 */
function renderDirectoryFields(inspection: DeployProjectInspection): string {
  const detection = detectSdkworkProject(inspection);
  return renderToStaticMarkup(createElement(DeployProjectDirectoryFields, {
    directory: inspection.rootPath,
    buildOutputPath: "",
    detection,
    inspecting: false,
    selectedSurface: undefined,
    matchedSurfacePath: undefined,
    buildOutputDetected: undefined,
    buildCandidates: [],
    t: zh,
    onDirectoryChange: () => {},
    onBuildOutputChange: () => {},
    onChangeDirectoryClick: () => {},
    onReinspect: () => {},
  }));
}

describe("DeployProjectDirectoryFields — 检测面板只在 sdkwork 项目下渲染", () => {
  it("非 sdkwork 目录：不再输出规范判定面板", () => {
    const markup = renderDirectoryFields(PLAIN_INSPECTION);

    for (const removed of ["未识别为", "规范目录标记", "检测到的应用表面", "未在 apps/ 下找到"]) {
      expect(markup).not.toContain(removed);
    }
  });

  it("sdkwork 项目：面板保留（含符合度徽标与规范目录标记）", () => {
    const markup = renderDirectoryFields(SDKWORK_INSPECTION);

    expect(markup).toContain(zh("detectionConformant"));
    expect(markup).toContain(zh("detectionMarkers"));
  });
});
