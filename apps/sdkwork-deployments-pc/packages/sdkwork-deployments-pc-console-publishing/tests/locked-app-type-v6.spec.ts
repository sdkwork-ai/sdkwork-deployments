// WORKSPACE-PATH:allow-fixture - this file is a test fixture that simulates a foreign
// checkout root, so the sdkwork-<name> segment below is the value under assertion rather
// than a binding to a real sibling checkout. PORTABILITY_SPEC.md section 5.2 governs it.
/**
 * v6 回归测试：发布阶段的应用类型**不可修改**。
 *
 * 需求原文：「新增应用已经明确了类型了，发布应用就不需要再选择类型，
 * 而且不允许修改的。」
 *
 * 因此这里锁三件事：
 *
 *   1. **反查是正向映射的逆**：`deploy_app.app_kind` → 卡片 → 同一 appKind。
 *      这条断言的存在理由是防漂移 —— 发布阶段用 `cardsOfAppKind` 反查表面根，
 *      创建阶段用 `appKindOfCard` 正向落库。两者一旦不再互为逆映射，应用就会
 *      被发布到**另一个平台的表面根**上（例如把 STATIC_WEB 发到 SPA_WEB 的
 *      `apps/*-h5`），而 UI 上完全看不出来。
 *   2. **只读区里没有写入路径**：`LockedAppTypeField` 的静态标记中不得出现
 *      `radiogroup` / `onChange` 语义的 9 卡网格，也不得出现任何「改为其他
 *      appKind」的控件。唯一的交互是 SPA_WEB 两张等价卡之间的**表面**选择，
 *      它不改 appKind。
 *   3. **对话框不再渲染类型网格**：`CreateDeployAppDialog` 源码中不得再引用
 *      `DeployAppTypeGrid`。这是源码级断言，因为该组件的可用性缺陷（可点、
 *      可改）恰恰在静态标记里会「看起来正常」。
 */
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { publishingTranslator } from "../src/i18n.ts";
import {
  appKindOfCard,
  cardIdsOfAppKind,
  cardsOfAppKind,
  CARD_APP_KIND,
  DEPLOY_APP_TYPE_CARDS,
  DEPLOY_APP_TYPE_OPTIONS,
  type DeployAppKind,
} from "../src/service/deploy-app-publishing.ts";
import { LockedAppTypeField } from "../src/components/LockedAppTypeField.tsx";

const zh = publishingTranslator("zh-CN");

const HERE = dirname(fileURLToPath(import.meta.url));
const OPTION_BY_ID = new Map(DEPLOY_APP_TYPE_OPTIONS.map((option) => [option.id, option]));

/** 渲染只读区（真实组件 + 真实反查结果，不 mock）。 */
function renderLocked(appKind: DeployAppKind | undefined, cardId?: string): string {
  return renderToStaticMarkup(createElement(LockedAppTypeField, {
    cards: cardsOfAppKind(appKind),
    appKind,
    cardId,
    onSelectCard: () => {},
    t: zh,
  }));
}

describe("v6 应用类型反查（appKind → 卡片）", () => {
  it("每张卡片的 appKind 都能反查回同一张卡（正向/反向互为逆映射）", () => {
    for (const card of DEPLOY_APP_TYPE_CARDS) {
      const kind = appKindOfCard(card.id);
      expect(kind, `card ${card.id} 未登记 appKind`).toBeDefined();
      // 反查必须包含原卡 —— 否则发布阶段会落到别的表面根上。
      expect(cardIdsOfAppKind(kind as DeployAppKind)).toContain(card.id);
    }
  });

  it("CARD_APP_KIND 的键集合与卡片网格完全一致（无孤儿、无遗漏）", () => {
    const cardIds = DEPLOY_APP_TYPE_CARDS.map((card) => card.id).sort();
    const tableIds = Object.keys(CARD_APP_KIND).sort();
    // 少一个键 ⇒ 某类应用发布时会反查不到卡片（类型显示为原始枚举）；
    // 多一个键 ⇒ 指向一张不存在的卡片，反查结果为空。
    expect(tableIds).toEqual(cardIds);
  });

  it("每张卡片的 appKind 与其解析行（option）的 appKind 一致", () => {
    // metadata.surface 取自卡片，而 platform target 的 platform 取自 option。
    // 两者若不一致，同一次发布会写出一对互相矛盾的事实。
    for (const card of DEPLOY_APP_TYPE_CARDS) {
      const framework = card.frameworks.find((candidate) => candidate.id === card.defaultFrameworkId);
      const option = framework === undefined
        ? undefined
        : OPTION_BY_ID.get(framework.optionId);
      expect(option, `card ${card.id} 的默认框架未解析出行`).toBeDefined();
      expect(option?.appKind, `card ${card.id} 的卡片/解析行 appKind 不一致`)
        .toBe(appKindOfCard(card.id));
    }
  });

  it("SPA_WEB 对应 h5 与 pc-web 两张卡（契约分不清表面，必须让用户指明）", () => {
    const spa = cardsOfAppKind("SPA_WEB");
    expect(spa.map((card) => card.id).sort()).toEqual(["h5", "pc-web"]);
    // 两张卡的 appKind 相同（所以在只读区里切换它们**不是**改应用类型），
    // 但表面根不同 —— 这正是必须保留这个选择的原因。
    const surfaces = new Set(spa.map((card) => card.surface));
    expect(surfaces.size).toBe(2);
  });

  it("未识别 / 缺省的 appKind 反查为空，而不是猜一张卡", () => {
    expect(cardsOfAppKind(undefined)).toEqual([]);
    // DOUYIN_MINIPROGRAM 在契约里存在，但发布器的 9 张卡里没有对应卡片：
    // 猜测一张会把抖音小程序发到微信小程序的表面根上。
    expect(cardsOfAppKind("DOUYIN_MINIPROGRAM")).toEqual([]);
  });
});

describe("v6 只读类型区不提供写入路径", () => {
  it("单候选（ANDROID_APP）只渲染一张只读卡，且带「固定」徽标", () => {
    const html = renderLocked("ANDROID_APP");
    expect(html).toContain(zh("typeLockedBadge"));
    expect(html).toContain(zh("typeAndroid"));
    // 单候选不该出现表面选择（没有第二个等价卡）。
    expect(html).not.toContain(zh("appTypeLockedAmbiguous"));
  });

  it("只读卡不是 button，也不带 radiogroup —— 不可点", () => {
    const html = renderLocked("ANDROID_APP");
    // 只读展示必须用 div 承载；出现 <button 就意味着有一个能点的控件。
    expect(html).not.toContain("<button");
    expect(html).not.toContain("radiogroup");
    expect(html).not.toContain("role=\"radio\"");
  });

  it("SPA_WEB 渲染两张等价卡（表面选择），但都不改 appKind", () => {
    const html = renderLocked("SPA_WEB", "h5");
    // 两张卡都在，供用户指明表面。
    expect(html).toContain(zh("typeH5"));
    expect(html).toContain(zh("typePcWeb"));
    expect(html).toContain(zh("appTypeLockedAmbiguous"));
    // 表面选择是 radiogroup 的 radio（两张等价卡之间二选一），
    // 但它选择的单位是卡片 id（= 表面根），不是 appKind。
    expect(html).toContain("radiogroup");
    const spa = cardsOfAppKind("SPA_WEB");
    for (const card of spa) {
      expect(appKindOfCard(card.id)).toBe("SPA_WEB");
    }
  });

  it("未识别的 appKind 展示原始枚举并提示重建，且不渲染卡片", () => {
    const html = renderLocked("DOUYIN_MINIPROGRAM");
    expect(html).toContain("DOUYIN_MINIPROGRAM");
    expect(html).toContain(zh("appTypeLockedUnknown"));
    expect(html).not.toContain("<button");
  });
});

describe("v6 CreateDeployAppDialog 不再渲染可编辑的类型网格", () => {
  const dialogSource = readFileSync(
    resolve(HERE, "../src/components/CreateDeployAppDialog.tsx"),
    "utf8",
  );

  it("源码中不引用 DeployAppTypeGrid（可点 9 卡网格已从发布流程移除）", () => {
    expect(dialogSource).not.toContain("DeployAppTypeGrid");
  });

  it("源码中用 LockedAppTypeField 承载第 1 步", () => {
    expect(dialogSource).toContain("LockedAppTypeField");
  });

  it("源码中保留 classifyAppTypeCards 会意味着项目仍在否决应用类型 —— 必须已移除", () => {
    // v4 的规则是「项目 apps/ 里没有的表面 → 置灰/清空选择」。v6 里应用类型
    // 由已建应用决定，这条规则若还在，一次目录变更就能把应用身份抹掉。
    expect(dialogSource).not.toContain("classifyAppTypeCards");
    expect(dialogSource).not.toContain("cardAvailability");
  });

  it("类型选择的种子来自 publishApp.appKind（而非用户输入）", () => {
    expect(dialogSource).toContain("cardsOfAppKind(publishApp?.appKind)");
  });
});
