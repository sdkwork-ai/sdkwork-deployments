import { describe, expect, it } from "vitest";

import { relativeRecordName } from "../packages/sdkwork-deployments-pc-console-delivery/src/dns-record-name.ts";

// The cases mirror the corpus of the service-side reduction
// (`dns_relative_record_name` in the ACME engine): the console row and the
// provider call must agree on what "inside the zone" means, or the operator
// publishes a record under the zone the provider did not expect.
describe("dns record name reduction", () => {
  it("reduces the verification label against the zone apex", () => {
    expect(relativeRecordName("_sdkwork-verification.birdcoder.com", "birdcoder.com")).toBe(
      "_sdkwork-verification",
    );
  });

  it("keeps the intermediate labels of a subdomain", () => {
    expect(relativeRecordName("_sdkwork-verification.shop.example.com", "example.com")).toBe(
      "_sdkwork-verification.shop",
    );
    expect(relativeRecordName("_sdkwork-verification.eu.example.com", "eu.example.com")).toBe(
      "_sdkwork-verification",
    );
  });

  it("does not reduce past a zone the record is not inside", () => {
    expect(relativeRecordName("_sdkwork-verification.other.com", "example.com")).toBeUndefined();
    expect(relativeRecordName("_sdkwork-verification.example.com", "com")).toBe(
      "_sdkwork-verification.example",
    );
  });

  it("refuses rather than guesses when the answer would be unusable", () => {
    // A record that *is* the zone has no prefix to type; the provider's own
    // apex row is the right place for it.
    expect(relativeRecordName("example.com", "example.com")).toBeUndefined();
    expect(relativeRecordName("_sdkwork-verification.example.com", undefined)).toBeUndefined();
    expect(relativeRecordName("_sdkwork-verification.example.com", "")).toBeUndefined();
  });

  it("folds case, trailing dots, and full-width stops on both sides", () => {
    expect(relativeRecordName("_SDKWORK-VERIFICATION.Example.COM.", " Example.com ")).toBe(
      "_sdkwork-verification",
    );
    expect(relativeRecordName("_sdkwork-verification。example。com", "example.com")).toBe(
      "_sdkwork-verification",
    );
  });
});
