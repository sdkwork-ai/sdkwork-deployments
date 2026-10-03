import { describe, expect, it } from "vitest";

import { marketplaceTranslator, translateMarketplace, type MarketplaceMessageKey } from "../src/i18n.ts";

describe("marketplace i18n", () => {
  it("renders both locales for every key without placeholders leaking", () => {
    const samples: MarketplaceMessageKey[] = [
      "marketplace.title",
      "marketplace.acquire",
      "marketplace.acquirePending",
      "myTemplates.create",
      "myTemplates.submit",
      "common.confirm",
    ];
    for (const key of samples) {
      const en = translateMarketplace("en-US", key);
      const zh = translateMarketplace("zh-CN", key);
      expect(en.length).toBeGreaterThan(0);
      expect(zh.length).toBeGreaterThan(0);
      expect(en).not.toMatch(/\{[a-zA-Z]+\}/);
      expect(zh).not.toMatch(/\{[a-zA-Z]+\}/);
    }
  });

  it("interpolates values into both locales", () => {
    expect(translateMarketplace("en-US", "marketplace.installCount", { count: 12 })).toContain("12");
    expect(translateMarketplace("zh-CN", "marketplace.installCount", { count: 12 })).toContain("12");
  });

  it("labels FREE and PAID pricing distinctly per locale", () => {
    const en = marketplaceTranslator("en-US");
    const zh = marketplaceTranslator("zh-CN");
    expect(en("marketplace.pricing.FREE")).not.toBe(en("marketplace.pricing.PAID"));
    expect(zh("marketplace.pricing.FREE")).toBe("免费");
    expect(zh("marketplace.pricing.PAID")).toBe("付费");
  });
});
