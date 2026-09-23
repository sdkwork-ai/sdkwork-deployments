/**
 * v7 回归测试：发布校验从「一句必填提示」变为**字段级**。
 *
 * 需求原文：「点击下一步：请先填写必填项再发布。在界面交互中，对应哪些 form
 * 表单必填和报错没有提示，用户看了错误不知道错误发生在哪里。」
 *
 * 锁四件事：
 *
 *   1. **判据落到具体字段**：`validatePublishFields` 是纯函数，这里直接执行它
 *      —— 每种缺失都必须映射到它自己的字段与文案键，不能退化成「集合里有没有
 *      东西」。这条断言在组件里是看不见的（渲染分支里的 `if` 属单测盲区）。
 *   2. **存在性与产物检查归到对应字段**：v7 之前它们是步骤级字符串（用户不知
 *      道该改哪个框），现在必须是 `directory` / `buildOutput` 上的问题。
 *   3. **快照覆盖的完整性**：`PUBLISH_FIELD_STEP` 与 `PUBLISH_FIELD_LABEL_KEYS`
 *      必须对每个 `PublishFieldId` 都有登记 —— 新加字段时漏登记会让「定位到
 *      出错字段」静默失效（跳回 undefined 步骤）。
 *   4. **界面真的标出来**：必填标记出现在必填字段上；首屏（未提交）不该有
 *      任何错误行 —— 错误只在用户动作之后出现。
 */
import { createElement, type ComponentProps } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { publishingTranslator } from "../src/i18n.ts";
import { CreateDeployAppDialog } from "../src/components/CreateDeployAppDialog.tsx";
import {
  PUBLISH_FIELD_LABEL_KEYS,
  PUBLISH_FIELD_STEP,
  publishProblemsSummary,
  validatePublishFields,
  type PublishFieldId,
  type PublishFieldSnapshot,
} from "../src/service/publish-field-validation.ts";

const zh = publishingTranslator("zh-CN");
const en = publishingTranslator("en-US");

const HERE = dirname(fileURLToPath(import.meta.url));

/** 一份「什么都填好了」的快照，便于逐项制造缺失。 */
const COMPLETE: PublishFieldSnapshot = {
  directory: "/repo/apps/sdkwork-demo-h5",
  buildOutputPath: "dist/standalone/dev",
  buildOutputDetected: true,
  frameworkId: "vite",
  targetAppId: "app-1",
  version: "1.0.0",
  inspectionAvailable: true,
  inspecting: false,
  inspected: true,
}

const ALL_FIELDS: readonly PublishFieldId[] = [
  "directory",
  "buildOutput",
  "framework",
  "targetApp",
  "version",
]

describe("v7 字段级校验：每种缺失都落到自己的字段", () => {
  it("空目录 → directory 自己报「请填写或选择源码目录」", () => {
    const problems = validatePublishFields(1, { ...COMPLETE, directory: undefined })
    expect(problems.directory?.messageKey).toBe("fieldErrorSourceDirectory")
    expect(Object.keys(problems)).toEqual(["directory"])
  })

  it("空产物目录 → buildOutput（不是 directory）", () => {
    const problems = validatePublishFields(1, { ...COMPLETE, buildOutputPath: "   " })
    expect(problems.buildOutput?.messageKey).toBe("fieldErrorBuildOutput")
    expect(Object.keys(problems)).toEqual(["buildOutput"])
  })

  it("未选框架 → framework", () => {
    const problems = validatePublishFields(1, { ...COMPLETE, frameworkId: undefined })
    expect(problems.framework?.messageKey).toBe("fieldErrorFramework")
    expect(Object.keys(problems)).toEqual(["framework"])
  })

  it("三样都缺 → 三个字段各自成立（不是一句概括）", () => {
    const problems = validatePublishFields(1, {
      ...COMPLETE,
      directory: undefined,
      buildOutputPath: "",
      frameworkId: undefined,
    })
    expect(Object.keys(problems).sort()).toEqual(["buildOutput", "directory", "framework"])
  })

  it("目录列举中 → directory 提示「正在检查」，而不是「目录不存在」", () => {
    const problems = validatePublishFields(1, { ...COMPLETE, inspecting: true })
    expect(problems.directory?.messageKey).toBe("publishCheckingDirectories")
  })

  it("目录列举失败 → directory 报「源码目录不存在」", () => {
    const problems = validatePublishFields(1, { ...COMPLETE, inspected: false })
    expect(problems.directory?.messageKey).toBe("sourceDirectoryMissing")
  })

  it("产物目录列举过但不存在 → buildOutput 带上路径参数", () => {
    const problems = validatePublishFields(1, { ...COMPLETE, buildOutputDetected: false })
    expect(problems.buildOutput?.messageKey).toBe("buildOutputMissing")
    expect(problems.buildOutput?.params?.path).toBe("dist/standalone/dev")
  })

  it("宿主没有列举能力时不做存在性判定（放行，由发布兜底）", () => {
    const problems = validatePublishFields(1, {
      ...COMPLETE,
      inspectionAvailable: false,
      inspected: false,
      buildOutputDetected: undefined,
    })
    expect(problems).toEqual({})
  })

  it("第 2 步：缺目标应用 → targetApp", () => {
    expect(validatePublishFields(2, { ...COMPLETE, targetAppId: undefined }).targetApp?.messageKey)
      .toBe("fieldErrorTargetApp")
    expect(validatePublishFields(2, COMPLETE)).toEqual({})
  })

  it("第 4 步：版本非法 → version；合法 → 无问题", () => {
    expect(validatePublishFields(4, { ...COMPLETE, version: "1.0" }).version?.messageKey).toBe("versionError")
    expect(validatePublishFields(4, { ...COMPLETE, version: "2.3.4" })).toEqual({})
  })

  it("第 3 步（应用资料）没有必填项", () => {
    expect(validatePublishFields(3, { ...COMPLETE, directory: undefined, version: "x" })).toEqual({})
  })
});

describe("v7 汇总与登记完整性", () => {
  it("页脚汇总指名道姓，不再只说「必填项」", () => {
    const problems = validatePublishFields(1, { ...COMPLETE, directory: undefined, frameworkId: undefined })
    expect(publishProblemsSummary(problems, zh("fieldSeparator"), zh))
      .toBe("还有 2 项必填未完成：源码目录、框架 / 架构")
    expect(publishProblemsSummary({}, zh("fieldSeparator"), zh)).toBeUndefined()
  })

  it("汇总随语言切换（错误状态本身语言无关）", () => {
    const problems = validatePublishFields(2, { ...COMPLETE, targetAppId: undefined })
    const summary = publishProblemsSummary(problems, en("fieldSeparator"), en)
    expect(summary).toContain("Target application")
    expect(summary).toContain("1")
  })

  it("每个字段都登记了步骤与标签（漏登记会让定位静默失效）", () => {
    for (const field of ALL_FIELDS) {
      expect(PUBLISH_FIELD_STEP[field]).toBeGreaterThanOrEqual(1)
      expect(PUBLISH_FIELD_STEP[field]).toBeLessThanOrEqual(4)
      expect(PUBLISH_FIELD_LABEL_KEYS[field]).toBeTruthy()
    }
    // 字段全集与登记表必须一一对应：多登记一个字段说明有字段被改了名。
    expect(Object.keys(PUBLISH_FIELD_STEP).sort()).toEqual([...ALL_FIELDS].sort())
    expect(Object.keys(PUBLISH_FIELD_LABEL_KEYS).sort()).toEqual([...ALL_FIELDS].sort())
  })
});

describe("v7 界面：必填被标出来，错误只在动作之后出现", () => {
  function renderFirstFrame(): string {
    const props = {
      deployClient: {},
      driveClient: {},
      locale: "zh-CN",
      publishApp: { id: "app-1", name: "示例应用", appKind: "ANDROID_APP", slug: "example-app" },
      onClose: () => {},
    } as unknown as ComponentProps<typeof CreateDeployAppDialog>;
    return renderToStaticMarkup(createElement(CreateDeployAppDialog, props));
  }

  it("首屏三个必填字段都带「必填」标记", () => {
    const html = renderFirstFrame();
    const requiredCount = html.split(zh("fieldRequired")).length - 1;
    // 源码目录 / 构建产物目录 / 框架（版本在第 4 步，首屏不渲染）。
    expect(requiredCount).toBe(3);
  });

  it("首屏没有任何错误行（未提交就不该红）", () => {
    const html = renderFirstFrame();
    expect(html).not.toContain(zh("fieldErrorSourceDirectory"));
    expect(html).not.toContain(zh("fieldErrorBuildOutput"));
    expect(html).not.toContain(zh("fieldErrorFramework"));
    expect(html).not.toContain("data-field-invalid=\"true\"");
  });

  it("字段容器带得动定位属性，且主按钮不再因校验而禁用", () => {
    const source = readFileSync(resolve(HERE, "../src/components/CreateDeployAppDialog.tsx"), "utf8");
    // 聚焦第一个出错字段靠这个属性找锚点。
    expect(source).toContain("data-field-invalid");
    // 主按钮只对 busy 禁用：灰着的按钮不解释为什么灰（实测反馈）。
    expect(source).toContain("disabled={busy} onClick={() => { void submit() }}");
    expect(source).not.toContain("disabled={busy || !canNext()}");
    // 字段级错误已接到子组件上。
    expect(source).toContain("directoryError={fieldErrorText(\"directory\")}");
    expect(source).toContain("buildOutputError={fieldErrorText(\"buildOutput\")}");
    expect(source).toContain("error={fieldErrorText(\"framework\")}");
    expect(source).toContain("error={fieldErrorText(\"targetApp\")}");
  });
});
