import { describe, expect, it } from "vitest";

import { marketplaceTranslator, translateMarketplace, type MarketplaceMessageKey } from "../src/i18n.ts";

describe("marketplace i18n", () => {
  it("renders both locales for every key without placeholders leaking", () => {
    const samples: MarketplaceMessageKey[] = [
      "marketplace.title",
      "marketplace.acquire",
      "marketplace.acquirePaid",
      "marketplace.acquired",
      "marketplace.openCashier",
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

  it("names the order in the payment-pending notice of both locales", () => {
    const en = translateMarketplace("en-US", "marketplace.paymentPending", { orderNo: "T-77" });
    const zh = translateMarketplace("zh-CN", "marketplace.paymentPending", { orderNo: "T-77" });
    expect(en).toContain("T-77");
    expect(zh).toContain("T-77");
    expect(en).not.toMatch(/\{[a-zA-Z]+\}/);
    expect(zh).not.toMatch(/\{[a-zA-Z]+\}/);
  });

  it("surfaces the provider payload in the QR notice of both locales", () => {
    const en = translateMarketplace("en-US", "marketplace.paymentQr", { orderNo: "T-77", qrCode: "weixin://pay/1" });
    const zh = translateMarketplace("zh-CN", "marketplace.paymentQr", { orderNo: "T-77", qrCode: "weixin://pay/1" });
    expect(en).toContain("weixin://pay/1");
    expect(zh).toContain("weixin://pay/1");
  });

  it("labels FREE and PAID pricing distinctly per locale", () => {
    const en = marketplaceTranslator("en-US");
    const zh = marketplaceTranslator("zh-CN");
    expect(en("marketplace.pricing.FREE")).not.toBe(en("marketplace.pricing.PAID"));
    expect(zh("marketplace.pricing.FREE")).toBe("免费");
    expect(zh("marketplace.pricing.PAID")).toBe("付费");
  });
});
