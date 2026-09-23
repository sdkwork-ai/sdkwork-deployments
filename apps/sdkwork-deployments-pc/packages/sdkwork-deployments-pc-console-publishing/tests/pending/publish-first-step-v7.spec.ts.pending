// WORKSPACE-PATH:allow-fixture - this file asserts on rendered markup of the
// publish dialog only; no foreign checkout root is referenced here.
/**
 * v7 回归测试：发布对话框打开即落在目录步骤，应用类型不再是一步。
 *
 * 需求原文：「发布应用中，因为新建应用中已经选择了应用类型，不需要显示应用
 * 类型了，直接走到第二步，而且不应该能回退到上一步。」
 *
 * 这里锁四件事：
 *
 *   1. **首屏就是目录步骤**（渲染级，非源码级）：用服务端渲染读取初始 state
 *      —— effect 不执行，所以看到的正是用户打开对话框的第一帧。它必须出现
 *      目录/产物字段，且不得出现「应用类型」只读区。只断言源码里的
 *      `useState(1)` 是不够的：初始 state 之后仍可能被 effect 改走。
 *   2. **唯一候选时没有任何类型 UI**：ANDROID_APP 渲染结果里既没有「应用类型」
 *      标签，也没有「固定」徽标 —— 连只读展示都不留。
 *   3. **多候选只问表面、不问类型**：SPA_WEB 渲染结果里出现「发布表面」与两张
 *      等价卡（H5 / PC 网页），但不出现「固定」徽标（类型磁贴整块不渲染）。
 *      这张卡是必要的：`cardsOfAppKind("SPA_WEB")` 有多张卡，不指明表面就
 *      解析不出 `type`，目录自动推导与框架列表都无从工作。
 *   4. **没有回到类型步骤的路径**：步骤总数 4，且「上一步」仅在 `step > 1`
 *      时渲染 —— 第 1 步（目录）没有上一步可点。
 */
import { createElement, type ComponentProps } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { publishingTranslator } from "../src/i18n.ts";
import { CreateDeployAppDialog } from "../src/components/CreateDeployAppDialog.tsx";
import { cardsOfAppKind } from "../src/service/deploy-app-publishing.ts";

const zh = publishingTranslator("zh-CN");

const HERE = dirname(fileURLToPath(import.meta.url));

/** 渲染发布对话框第一帧（真实组件，client 用空壳 —— 首帧不发请求）。 */
function renderFirstFrame(appKind: string): string {
  const props = {
    deployClient: {},
    driveClient: {},
    locale: "zh-CN",
    publishApp: { id: "app-1", name: "示例应用", appKind, slug: "example-app" },
    onClose: () => {},
  } as unknown as ComponentProps<typeof CreateDeployAppDialog>;
  return renderToStaticMarkup(createElement(CreateDeployAppDialog, props));
}

describe("v7 发布对话框首屏 = 目录步骤", () => {
  it("首屏渲染目录与产物字段（不是应用类型）", () => {
    const html = renderFirstFrame("ANDROID_APP");
    expect(html).toContain(zh("sourceDirectory"));
    expect(html).toContain(zh("buildOutputPath"));
  });

  it("唯一候选的应用类型：首屏不再出现任何类型 UI", () => {
    const html = renderFirstFrame("ANDROID_APP");
    expect(cardsOfAppKind("ANDROID_APP")).toHaveLength(1);
    // 「应用类型」标签与「固定」徽标都不该出现 —— 类型已定且无需再看一次。
    expect(html).not.toContain(zh("appTypeLocked"));
    expect(html).not.toContain(zh("typeLockedBadge"));
    expect(html).not.toContain(zh("typeAndroid"));
  });

  it("SPA_WEB：只问表面（发布表面 + 两张等价卡），不展示类型磁贴", () => {
    const html = renderFirstFrame("SPA_WEB");
    expect(cardsOfAppKind("SPA_WEB").length).toBeGreaterThan(1);
    expect(html).toContain(zh("publishSurface"));
    expect(html).toContain(zh("typeH5"));
    expect(html).toContain(zh("typePcWeb"));
    // 类型磁贴（带「固定」徽标）整块不渲染。
    expect(html).not.toContain(zh("typeLockedBadge"));
    // 多候选「尚未选表面」≠「类型未识别」：把两者混为一谈会给 SPA_WEB 应用
    // 打出「类型不在识别范围内、请重建应用」这种完全错误的指引。
    expect(html).not.toContain(zh("appTypeLockedUnknown"));
    expect(html).toContain(zh("appTypeLockedAmbiguous"));
  });

  it("未识别的 appKind：首屏给出可见解释，而不是静默禁用", () => {
    const html = renderFirstFrame("DOUYIN_MINIPROGRAM");
    expect(cardsOfAppKind("DOUYIN_MINIPROGRAM")).toHaveLength(0);
    expect(html).toContain(zh("appTypeLockedUnknown"));
  });
});

describe("v7 没有回到类型步骤的路径", () => {
  const dialogSource = readFileSync(
    resolve(HERE, "../src/components/CreateDeployAppDialog.tsx"),
    "utf8",
  );

  it("步骤总数为 4（类型步骤已整步移除）", () => {
    expect(dialogSource).toContain("const STEP_COUNT = 4");
  });

  it("「上一步」只在 step > 1 渲染 —— 第 1 步没有上一步", () => {
    expect(dialogSource).toContain("step > 1 &&");
    // previous 的下限是 1，不存在 step 0 / 负数步骤（否则用户会掉进空步骤）。
    expect(dialogSource).toContain("Math.max(1, current - 1)");
  });

  it("类型区只在确实需要用户输入时才渲染（多候选 / 未识别）", () => {
    expect(dialogSource).toMatch(/\(needsSurfaceChoice \|\| lockedCards\.length === 0\)/);
    // 仍然保留 LockedAppTypeField 的调用点：多候选时用户必须能指明表面。
    expect(dialogSource).toContain("LockedAppTypeField");
  });
});
