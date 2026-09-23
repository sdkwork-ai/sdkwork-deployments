import { describe, expect, it } from "vitest";

import {
  groupOwnershipRecords,
  rowMatchesFilterTab,
  scopeFilterTab,
} from "../packages/sdkwork-deployments-pc-console-delivery/src/DeliveryManagement.tsx";
import type { DomainHostnameClaimResponse, DomainHostnameResponse } from "@sdkwork/deployments-app-sdk";

// The panel used to be built claim by claim, and a wildcard certificate plans two
// identifiers into **one** DNS record: `*.example.com` and the apex it leaves out
// both fold to `_sdkwork-verification.example.com`. Two blocks naming the same
// record read as the panel having duplicated a row, which is what an operator
// reported — while what DNS actually wants is both TXT values published at that
// one name. These cases pin the fold, because the defect was invisible to every
// check the code had: the claims were correct, the values were correct, and only
// the grouping was wrong.
//
// The routing and filtering cases are here for the same reason: they are the two
// decisions the coverage picker makes, extracted so they can be driven without a
// browser.

const ZONE_ID = "11111111-1111-4111-8111-111111111111";

function firstLabel(name: string): string {
  return name.split(".")[0] ?? name;
}

function hostname(hostname: string, hostnameType: "EXACT" | "WILDCARD" = "EXACT"): DomainHostnameResponse {
  const relativeName = hostname.startsWith("*.")
    ? `*.${firstLabel(hostname.slice(2))}`
    : firstLabel(hostname);
  return {
    id: `${hostnameType}:${hostname}`,
    zoneId: ZONE_ID,
    hostname,
    relativeName: hostname === "example.com" ? "@" : relativeName,
    hostnameType,
    verificationStatus: "PENDING",
    status: "ACTIVE",
    certificateCount: "0",
    bindingCount: "0",
    createdAt: "2026-09-22T00:00:00.000Z",
    updatedAt: "2026-09-22T00:00:00.000Z",
    version: "1",
  };
}

function claim(
  name: string,
  hostnameType: "EXACT" | "WILDCARD",
  record: { name?: string; relativeName?: string; value?: string },
): DomainHostnameClaimResponse {
  return {
    hostname: hostname(name, hostnameType),
    verified: false,
    ...(record.name === undefined ? {} : { dnsRecordName: record.name }),
    ...(record.relativeName === undefined ? {} : { dnsRecordRelativeName: record.relativeName }),
    ...(record.value === undefined ? {} : { dnsRecordValue: record.value }),
    ...(record.name === undefined ? {} : { dnsRecordType: "TXT" as const }),
  };
}

describe("ownership records are grouped by DNS record, not by hostname", () => {
  it("folds a wildcard and the apex it leaves out into one record with two values", () => {
    const groups = groupOwnershipRecords([
      claim("*.example.com", "WILDCARD", {
        name: "_sdkwork-verification.example.com",
        relativeName: "_sdkwork-verification",
        value: "wildcard-digest",
      }),
      claim("example.com", "EXACT", {
        name: "_sdkwork-verification.example.com",
        relativeName: "_sdkwork-verification",
        value: "apex-digest",
      }),
    ], "example.com");

    expect(groups).toHaveLength(1);
    expect(groups[0]?.recordName).toBe("_sdkwork-verification.example.com");
    expect(groups[0]?.relativeName).toBe("_sdkwork-verification");
    // Both hostnames are named on the one record: a shared record that did not say
    // it was shared reads as a duplicate to be cleaned up.
    expect(groups[0]?.hostnames).toEqual(["*.example.com", "example.com"]);
    expect(groups[0]?.values).toEqual(["wildcard-digest", "apex-digest"]);
  });

  it("keeps genuinely different records apart", () => {
    const groups = groupOwnershipRecords([
      claim("*.example.com", "WILDCARD", {
        name: "_sdkwork-verification.example.com",
        relativeName: "_sdkwork-verification",
        value: "one",
      }),
      claim("*.shop.example.com", "WILDCARD", {
        name: "_sdkwork-verification.shop.example.com",
        relativeName: "_sdkwork-verification.shop",
        value: "two",
      }),
    ], "example.com");

    expect(groups.map((group) => group.recordName)).toEqual([
      "_sdkwork-verification.example.com",
      "_sdkwork-verification.shop.example.com",
    ]);
    expect(groups.map((group) => group.values)).toEqual([["one"], ["two"]]);
  });

  it("falls back to the local fold when the response predates the server-side one", () => {
    // No `dnsRecordRelativeName`: the relative name has to come from the record
    // name and the zone apex, which is the fold this panel used to do inline.
    const groups = groupOwnershipRecords([
      claim("*.shop.example.com", "WILDCARD", { name: "_sdkwork-verification.shop.example.com", value: "v1" }),
      claim("shop.example.com", "EXACT", { name: "_sdkwork-verification.shop.example.com", value: "v2" }),
    ], "example.com");

    expect(groups).toHaveLength(1);
    expect(groups[0]?.relativeName).toBe("_sdkwork-verification.shop");
    expect(groups[0]?.values).toEqual(["v1", "v2"]);
  });

  it("groups a response with no record name at all, and repeats no value", () => {
    const groups = groupOwnershipRecords([
      claim("example.com", "EXACT", { relativeName: "@", value: "same" }),
      claim("example.com", "EXACT", { relativeName: "@", value: "same" }),
      claim("example.com", "EXACT", { relativeName: "@", value: "other" }),
    ], "example.com");

    expect(groups).toHaveLength(1);
    expect(groups[0]?.hostnames).toEqual(["example.com"]);
    // Publishing the same digest twice is a duplicate TXT, not a second record.
    expect(groups[0]?.values).toEqual(["same", "other"]);
  });
});

describe("the coverage picker's filter follows the certificate type", () => {
  it("opens on the tab the certificate type can actually cover", () => {
    expect(scopeFilterTab("WILDCARD")).toBe("wildcard");
    expect(scopeFilterTab("SINGLE_DOMAIN")).toBe("exact");
  });

  it("offers only the names the open tab stands for", () => {
    const wildcard = hostname("*.example.com", "WILDCARD");
    const apex = hostname("example.com");
    const sub = hostname("api.example.com");

    expect(rowMatchesFilterTab(wildcard, "wildcard")).toBe(true);
    expect(rowMatchesFilterTab(apex, "wildcard")).toBe(false);
    expect(rowMatchesFilterTab(sub, "wildcard")).toBe(false);

    expect(rowMatchesFilterTab(apex, "exact")).toBe(true);
    expect(rowMatchesFilterTab(sub, "exact")).toBe(true);
    expect(rowMatchesFilterTab(wildcard, "exact")).toBe(false);

    // "All" is the union, and stays a union: it is the tab that would hide nothing
    // if the certificate type ever left it open.
    for (const row of [wildcard, apex, sub]) expect(rowMatchesFilterTab(row, "all")).toBe(true);
  });
});
