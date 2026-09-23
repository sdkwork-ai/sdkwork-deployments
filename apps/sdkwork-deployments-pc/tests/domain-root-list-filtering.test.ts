import { readFileSync } from "node:fs";
import { resolve } from "node:path";

import { describe, expect, it } from "vitest";

import { isRootDomainApex } from "../packages/sdkwork-deployments-pc-console-delivery/src/root-domain.ts";

// The root-domain page lists root domains. The platform provisions one
// `app.<suffix>` zone per suffix it serves for the whole tenant, and those rows
// arrive in the same listing as the operator's own root domains — which is what
// an operator reported, with the page showing `app.example.com` as if it were a
// root domain.
//
// What separates the two is shape, not ownership: an `app.<suffix>` apex is a
// subdomain, so `isRootDomainApex` answers false for it and true for the apex of
// every zone `create_domain_zone` would accept. The cases below are that
// decision table, and the last one pins the wiring — a filter that is present
// but unused reads as fixed while the page still shows the rows it was written
// to hide.

describe("root-domain list filtering", () => {
  it("accepts the apex a zone is registered under", () => {
    for (const hostname of ["example.com", "x.com", "sdkwork.cn", "example.co.uk", "example.com."]) {
      expect(isRootDomainApex(hostname), hostname).toBe(true);
    }
  });

  it("rejects the platform provisioning apexes", () => {
    for (const hostname of ["app.example.com", "app.sdkwork.cn", "app.birdcoder.com", "demo.app.example.com"]) {
      expect(isRootDomainApex(hostname), hostname).toBe(false);
    }
  });

  it("rejects what is not a registrable root domain at all", () => {
    for (const hostname of ["co.uk", "com", "localhost", "", "   ", "*.example.com"]) {
      expect(isRootDomainApex(hostname), hostname).toBe(false);
    }
  });

  it("filters the root-domain listing and keeps its count honest", () => {
    const source = readFileSync(
      resolve(import.meta.dirname, "../packages/sdkwork-deployments-pc-console-delivery/src/DeliveryManagement.tsx"),
      "utf8",
    );
    const start = source.indexOf("function DomainZoneList");
    const listing = source.slice(start, source.indexOf("function DomainHostnameList", start));
    expect(listing).toContain("isRootDomainApex(zone.apexHostname)");
    expect(listing).toContain("totalItems:");
  });
});
