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

/**
 * The cloud-account facet on the same listing.
 *
 * Three states reach the wire differently, and the one that is easiest to get
 * wrong is the unfiltered one: `ALL` is the page's own control value and has no
 * wire spelling, so it has to be an omission rather than a sentinel account id
 * nobody holds. `UNASSIGNED` is the opposite case — a real restriction that only
 * exists as the contract's reserved literal, so it has to be sent verbatim.
 *
 * The facet is asserted off the source rather than through a render because the
 * page is a hook-heavy container whose test harness lives in the hosting
 * application; what this pins is the request shape, which is what the hosting
 * application's own suite cannot see.
 */
describe("root-domain cloud-account facet", () => {
  const moduleSource = (): string =>
    readFileSync(
      resolve(import.meta.dirname, "../packages/sdkwork-deployments-pc-console-delivery/src/DeliveryManagement.tsx"),
      "utf8",
    );
  const listing = (): string => {
    const source = moduleSource();
    const start = source.indexOf("function DomainZoneList");
    return source.slice(start, source.indexOf("function DomainHostnameList", start));
  };

  it("omits the member for the unfiltered state and sends the reserved literal for unassigned", () => {
    const source = listing();
    expect(source).toContain('providerAccountId: cloudAccount === ALL_CLOUD_ACCOUNTS ? undefined : cloudAccount');
    // The two control values are module constants, not locals of the page.
    expect(moduleSource()).toContain('const ALL_CLOUD_ACCOUNTS = "ALL"');
    expect(moduleSource()).toContain('const UNASSIGNED_CLOUD_ACCOUNT = "UNASSIGNED"');
    // The literal is offered as its own option, so "the domains that resolve their
    // account per operation" is a question the operator can actually ask.
    expect(source).toContain('<option value={UNASSIGNED_CLOUD_ACCOUNT}>');
    expect(source).toContain('<option value={ALL_CLOUD_ACCOUNTS}>');
  });

  it("re-issues the listing when the facet changes", () => {
    // A control that renders but is not in the effect's dependency list filters
    // nothing: the read would keep answering the previous selection.
    expect(listing()).toContain(
      "}, [cloudAccount, keyword, page, pageSize, refreshVersion, service, status]);",
    );
  });

  it("offers the caller's own accounts rather than every account in the tenant", () => {
    const source = listing();
    // `mine` is the console's own reach: the tenant console is a personal surface,
    // so a platform-scope account — the deployment's infrastructure credential
    // rather than one this operator bound — must not appear as one of "my cloud
    // accounts". The operations surface is where the whole-tenant inventory belongs.
    expect(source).toContain("service.listCloudAccounts({ pageSize: 200, mine: true })");
  });

  it("keeps a pin the account center did not return selectable", () => {
    const source = listing();
    // Both the loaded rows and the current selection are folded into the options:
    // a `<select>` whose value matches no option silently shows its first entry,
    // and the row would then read a name the filter is not actually using.
    expect(source).toContain('zones.map((zone) => zone.providerAccountId ?? "")');
    expect(source).toContain("const bindings = [");
    expect(source).toContain("if (!byId.has(accountId)) byId.set(accountId,");
  });
});
