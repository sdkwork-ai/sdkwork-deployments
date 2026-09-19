/**
 * Unit tests for the per-app-kind store asset specifications.
 *
 * 覆盖三件事：
 *   1. 每个应用类型确实拿到**自己那一套**规范（这正是本轮需求）；
 *   2. 校验按 aspect → 像素下限的顺序判错，且给出可操作 reason；
 *   3. 商店硬约束（张数、必需档位、无截图语义的类型）不被违反。
 */
import { describe, expect, it } from "vitest";
import {
  APP_ICON_SPEC,
  APP_STORE_PREVIEW_TARGETS,
  COVER_SPEC,
  MAX_SCREENSHOTS_TOTAL,
  MEDIA_ACCEPTED_TYPES,
  PREVIEW_ASPECT_TOLERANCE,
  countScreenshots,
  mediaSpecForAppKind,
  missingRequiredScreenshots,
  previewTargetsForAppKind,
  validateCover,
  validateIcon,
  validatePreviewSize,
} from "../src/service/app-store-preview-spec.ts";
import type { DeployAppKind } from "../src/service/deploy-app-publishing.ts";

/** 契约里全部应用类型 —— 保证新增成员时测试立刻报红。 */
const ALL_APP_KINDS: readonly DeployAppKind[] = [
  "STATIC_WEB",
  "SPA_WEB",
  "API_SERVICE",
  "WECHAT_MINIPROGRAM",
  "DOUYIN_MINIPROGRAM",
  "IOS_APP",
  "ANDROID_APP",
  "HARMONYOS_APP",
  "DESKTOP_APP",
];

describe("APP_STORE_PREVIEW_TARGETS (Apple shelves)", () => {
  it("leads with the 6.9\" shelf, which is the only mandatory iPhone size", () => {
    // Apple 自 2024 起只需 6.9" 一套，其余向下缩放 ⇒ 第一档必须是 6.9"。
    expect(APP_STORE_PREVIEW_TARGETS[0].key).toBe("iphone-69");
    expect(APP_STORE_PREVIEW_TARGETS[0].width).toBe(1320);
    expect(APP_STORE_PREVIEW_TARGETS[0].height).toBe(2868);
  });

  it("uses 2064x2752 for the required iPad 13\" shelf", () => {
    const ipad = APP_STORE_PREVIEW_TARGETS.find((target) => target.key === "ipad-13");
    expect(ipad?.width).toBe(2064);
    expect(ipad?.height).toBe(2752);
  });

  it("caps every Apple shelf at 10 screenshots and requires at least 3", () => {
    for (const target of APP_STORE_PREVIEW_TARGETS) {
      expect(target.max).toBe(10);
      expect(target.min).toBe(3);
      expect(target.store).toBe("app-store");
    }
  });
});

describe("mediaSpecForAppKind — 不同类型不同规范", () => {
  it("resolves a spec for every contract app kind", () => {
    for (const kind of ALL_APP_KINDS) {
      const spec = mediaSpecForAppKind(kind);
      expect(spec.appKind).toBe(kind);
      expect(spec.icon.recommendedEdge).toBeGreaterThan(0);
      expect(spec.cover.recommendedWidth).toBeGreaterThan(0);
    }
  });

  it("gives Android apps Google Play shelves, not App Store shelves", () => {
    const spec = mediaSpecForAppKind("ANDROID_APP");
    expect(spec.screenshots.every((target) => target.store === "google-play")).toBe(true);
    expect(spec.screenshots.map((target) => target.key)).toContain("android-phone");
    // 1080x1920 是 Google Play 手机档的标准尺寸。
    const phone = spec.screenshots.find((target) => target.key === "android-phone");
    expect({ width: phone?.width, height: phone?.height }).toEqual({ width: 1080, height: 1920 });
  });

  it("does NOT leak App Store shelves into Android or web specs", () => {
    for (const kind of ["ANDROID_APP", "SPA_WEB", "STATIC_WEB", "DESKTOP_APP"] as const) {
      const keys = mediaSpecForAppKind(kind).screenshots.map((target) => target.key);
      expect(keys.some((key) => key.startsWith("iphone-"))).toBe(false);
    }
  });

  it("gives web kinds a single 1280x800 landscape shelf", () => {
    const spec = mediaSpecForAppKind("SPA_WEB");
    expect(spec.screenshots).toHaveLength(1);
    expect(spec.screenshots[0]!.width).toBe(1280);
    expect(spec.screenshots[0]!.height).toBe(800);
    // 网页列表页截图是可选的。
    expect(spec.screenshotsRequired).toBe(false);
  });

  it("gives API services no screenshot shelf at all", () => {
    const spec = mediaSpecForAppKind("API_SERVICE");
    expect(spec.screenshots).toHaveLength(0);
    expect(spec.screenshotsRequired).toBe(false);
    expect(spec.maxScreenshotsTotal).toBe(0);
    // 图标与封面仍然适用。
    expect(spec.icon.mustBeSquare).toBe(true);
    expect(previewTargetsForAppKind("API_SERVICE")).toHaveLength(0);
  });

  it("gives desktop apps a 16:10 Mac shelf and a 16:9 desktop shelf", () => {
    const spec = mediaSpecForAppKind("DESKTOP_APP");
    const keys = spec.screenshots.map((target) => target.key);
    expect(keys).toEqual(["mac", "desktop"]);
    const mac = spec.screenshots.find((target) => target.key === "mac");
    expect(mac?.width).toBe(2880);
    expect(mac?.height).toBe(1800);
    expect(spec.screenshotsRequired).toBe(false);
  });

  it("gives mini-program kinds only their own store shelf", () => {
    for (const kind of ["WECHAT_MINIPROGRAM", "DOUYIN_MINIPROGRAM"] as const) {
      const spec = mediaSpecForAppKind(kind);
      const keys = spec.screenshots.map((target) => target.key);
      // 小程序不上架 Chrome Web Store —— 只应有自己的档位，不夹带浏览器扩展。
      expect(keys).toHaveLength(1);
      expect(keys.some((key) => key === "browser-extension")).toBe(false);
      for (const target of spec.screenshots) {
        expect(target.store).not.toBe("chrome-web-store");
      }
      expect(spec.screenshotsRequired).toBe(true);
      expect(spec.cover.recommendedHeight).toBeLessThan(spec.cover.recommendedWidth);
      expect(spec.icon.mustBeSquare).toBe(true);
      expect(spec.icon.minEdge).toBe(144);
    }
    expect(mediaSpecForAppKind("WECHAT_MINIPROGRAM").noteKey).toBe("mediaSpecWechatMiniProgram");
    expect(mediaSpecForAppKind("DOUYIN_MINIPROGRAM").noteKey).toBe("mediaSpecDouyinMiniProgram");
  });

  it("falls back to the web spec for an unknown kind", () => {
    expect(mediaSpecForAppKind(undefined).appKind).toBe("SPA_WEB");
  });

  it("never exceeds the global screenshot cap", () => {
    for (const kind of ALL_APP_KINDS) {
      expect(mediaSpecForAppKind(kind).maxScreenshotsTotal).toBeLessThanOrEqual(MAX_SCREENSHOTS_TOTAL);
    }
  });
});

describe("validatePreviewSize — aspect-first", () => {
  const iphone69 = APP_STORE_PREVIEW_TARGETS[0];

  it("accepts the exact nominal size", () => {
    expect(validatePreviewSize(1320, 2868, iphone69)).toEqual({ ok: true });
  });

  it("accepts sizes within the aspect tolerance", () => {
    const shifted = Math.round(1320 * (1 - PREVIEW_ASPECT_TOLERANCE / 2));
    expect(validatePreviewSize(shifted, 2868, iphone69)).toEqual({ ok: true });
  });

  it("reports 'aspect' (not 'size') for a transposed image", () => {
    const verdict = validatePreviewSize(2868, 1320, iphone69);
    expect(verdict.ok).toBe(false);
    if (!verdict.ok) {
      expect(verdict.reason).toBe("aspect");
      expect(verdict.detail).toContain("2868");
    }
  });

  it("accepts both 9:16 and 16:9 on the Google Play phone shelf", () => {
    const phone = mediaSpecForAppKind("ANDROID_APP").screenshots
      .find((target) => target.key === "android-phone")!;
    // Google 接受 9:16 → 16:9 的宽带，而不是单一比例。
    expect(validatePreviewSize(1080, 1920, phone).ok).toBe(true);
    expect(validatePreviewSize(1920, 1080, phone).ok).toBe(true);
    // 明显超出 2:1 的超宽图应被拒。
    expect(validatePreviewSize(2400, 800, phone).ok).toBe(false);
  });

  it("rejects a 16:9 image on a 9:16-only shelf", () => {
    const web = mediaSpecForAppKind("SPA_WEB").screenshots[0]!;
    // 16:9 塞进 16:10 档位 → 比例已超出 ±2% 带。
    expect(validatePreviewSize(1920, 1080, web).ok).toBe(false);
    // 正确的 16:10 通过。
    expect(validatePreviewSize(1280, 800, web).ok).toBe(true);
  });
});

describe("validateIcon", () => {
  it("requires a square image when the spec says so", () => {
    expect(validateIcon(1024, 1024, APP_ICON_SPEC).ok).toBe(true);
    const verdict = validateIcon(1024, 768, APP_ICON_SPEC);
    expect(verdict.ok).toBe(false);
    if (!verdict.ok) expect(verdict.reason).toBe("size");
  });

  it("rejects an icon below the minimum edge", () => {
    expect(validateIcon(256, 256, APP_ICON_SPEC).ok).toBe(false);
  });

  it("requires at least 144x144 for a mini-program icon", () => {
    const spec = mediaSpecForAppKind("WECHAT_MINIPROGRAM");
    // 128 是浏览器扩展的门槛，不是小程序的门槛。
    expect(validateIcon(128, 128, spec.icon).ok).toBe(false);
    expect(validateIcon(144, 144, spec.icon).ok).toBe(true);
    expect(validateIcon(512, 512, spec.icon).ok).toBe(true);
  });
});

describe("validateCover", () => {
  it("accepts the recommended Google Play feature graphic size", () => {
    const spec = mediaSpecForAppKind("ANDROID_APP");
    expect(validateCover(1024, 500, spec.cover).ok).toBe(true);
  });

  it("rejects a square image as a landscape banner", () => {
    const spec = mediaSpecForAppKind("SPA_WEB");
    const verdict = validateCover(1200, 1200, spec.cover);
    expect(verdict.ok).toBe(false);
    if (!verdict.ok) expect(verdict.reason).toBe("aspect");
  });

  it("rejects a banner below the pixel floor even when the ratio is right", () => {
    const spec = mediaSpecForAppKind("SPA_WEB");
    // 16:9 比例正确，但远小于 1200x630 下限。
    const verdict = validateCover(320, 180, spec.cover);
    expect(verdict.ok).toBe(false);
    if (!verdict.ok) expect(verdict.reason).toBe("size");
  });

  it("keeps the default exported cover spec consistent with the web spec", () => {
    expect(COVER_SPEC.recommendedWidth).toBe(mediaSpecForAppKind("SPA_WEB").cover.recommendedWidth);
  });
});

describe("missingRequiredScreenshots", () => {
  it("names every shelf still below its store minimum", () => {
    const spec = mediaSpecForAppKind("IOS_APP");
    const missing = missingRequiredScreenshots(spec, {});
    // iPhone 各档位都要求至少 3 张，iPad 同理。
    expect(missing.length).toBe(spec.screenshots.length);

    const satisfied = Object.fromEntries(
      spec.screenshots.map((target) => [target.key, Array.from({ length: target.min }, () => ({}))]),
    );
    expect(missingRequiredScreenshots(spec, satisfied)).toEqual([]);
  });

  it("requires nothing for kinds whose screenshots are optional", () => {
    for (const kind of ["SPA_WEB", "STATIC_WEB", "API_SERVICE", "DESKTOP_APP"] as const) {
      expect(missingRequiredScreenshots(mediaSpecForAppKind(kind), {})).toEqual([]);
    }
  });
});

describe("countScreenshots", () => {
  it("sums every shelf", () => {
    expect(countScreenshots({ a: [{}, {}], b: [{}], c: [] })).toBe(3);
    expect(countScreenshots({})).toBe(0);
  });
});

describe("MEDIA_ACCEPTED_TYPES", () => {
  it("accepts png, jpeg and webp", () => {
    expect(MEDIA_ACCEPTED_TYPES).toEqual(["image/png", "image/jpeg", "image/webp"]);
  });

  it("is wired into every app kind's spec", () => {
    for (const kind of ALL_APP_KINDS) {
      expect(mediaSpecForAppKind(kind).acceptedTypes).toEqual(MEDIA_ACCEPTED_TYPES);
    }
  });
});
