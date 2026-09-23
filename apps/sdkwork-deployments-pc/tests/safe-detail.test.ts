import { describe, expect, it } from "vitest";

import { safeErrorDetail } from "../packages/sdkwork-deployments-pc-console-delivery/src/safe-detail.ts";

// `lastErrorDetail` is the only text this console renders that we did not write:
// it is whatever a DNS provider or a CA answered, relayed through our API. These
// cases pin the two things that make it safe to put on screen — it is bounded,
// and it is withheld rather than mangled when it looks like a credential.
describe("provider refusal rendering", () => {
  it("passes a real refusal through unchanged so the operator reads the vendor's words", () => {
    // The three shapes that actually reach this cell in production: a DNSPod
    // status envelope, a Cloudflare authentication error, and an Aliyun
    // signature mismatch.
    expect(safeErrorDetail("DNSPod rejected Domain.Create: (-1) Domain not under your account")).toBe(
      "DNSPod rejected Domain.Create: (-1) Domain not under your account",
    );
    expect(safeErrorDetail("Cloudflare rejected the challenge record: (10000) Authentication error")).toBe(
      "Cloudflare rejected the challenge record: (10000) Authentication error",
    );
    expect(
      safeErrorDetail(
        "Aliyun rejected AddDomainRecord: InvalidAccessKeyId.NotFound (Specified access key is not found.)",
      ),
    ).toBe("Aliyun rejected AddDomainRecord: InvalidAccessKeyId.NotFound (Specified access key is not found.)");
  });

  it("keeps a Chinese refusal intact instead of losing it to a byte-wise cut", () => {
    // The regression this guards: a multi-byte character split by a byte-indexed
    // truncation used to turn the whole message into blanks, so the one language
    // most of this deployment's DNS refusals arrive in was the one that never
    // reached an operator.
    expect(safeErrorDetail("DNSPod 拒绝 Domain.Create：（-1）该域名不属于当前账号")).toBe(
      "DNSPod 拒绝 Domain.Create：（-1）该域名不属于当前账号",
    );
    // 400 double-byte characters is past the 320 ceiling, so it must come back
    // bounded and marked as truncated rather than cut mid-character.
    const long = "域".repeat(400);
    const bounded = safeErrorDetail(long);
    expect(bounded?.length).toBe(320);
    expect(bounded?.endsWith("...")).toBe(true);
    // The same input measured in bytes would be 1200, which is why a byte-indexed
    // bound would slice through a character and blank the message.
    expect(new TextEncoder().encode(long).length).toBe(1200);
  });

  it("collapses a multi-line provider body into the one line the cell can hold", () => {
    expect(safeErrorDetail("refused:\n  rule 1\r\n  rule 2\t")).toBe("refused: rule 1 rule 2");
  });

  it("withholds text that looks like a credential rather than showing a redacted half", () => {
    expect(safeErrorDetail("Authorization: Bearer abcdefghijklmnop")).toBeUndefined();
    expect(safeErrorDetail("api_key=abcdefghijklmnop")).toBeUndefined();
    expect(safeErrorDetail("traceback (most recent call last)")).toBeUndefined();
  });

  it("renders nothing for absent or blank text so no empty line appears under a code", () => {
    expect(safeErrorDetail(undefined)).toBeUndefined();
    expect(safeErrorDetail("   ")).toBeUndefined();
    expect(safeErrorDetail(42)).toBeUndefined();
  });
});
