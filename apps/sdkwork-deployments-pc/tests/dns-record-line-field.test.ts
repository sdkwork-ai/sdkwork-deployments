import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";

import { describe, expect, it } from "vitest";

// 解析线路 (recordLine) is the Aliyun-shaped "resolution line" field: the
// strip must carry it through form state, the create body, and the update
// body, or the operator types a line the wire never sends. The generated
// request type and the backend column are the authority; these checks pin the
// console wiring to them. Read as a source contract because the strip is a
// table-row form (no render-test harness in this app).

const APP_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");

const read = (relative: string): string =>
  readFileSync(resolve(APP_ROOT, relative), "utf8");

const DELIVERY = "packages/sdkwork-deployments-pc-console-delivery/src/DeliveryManagement.tsx";

describe("dns record line field on the console strip", () => {
  const delivery = read(DELIVERY);

  it("keeps recordLine in the form values, the seed, and the reset", () => {
    expect(delivery).toMatch(/interface DnsRecordFormValues \{[^}]*recordLine: string/s);
    expect(delivery).toMatch(/recordLine: ""/);
    expect(delivery).toMatch(/recordLine: record\.recordLine \?\? ""/);
  });

  it("sends recordLine on create and update, and omits it when empty", () => {
    const readForm = delivery.slice(
      delivery.indexOf("const readDnsForm"),
      delivery.indexOf("const submitAdd"),
    );
    expect(readForm).toContain("form.recordLine.trim()");
    expect(readForm).toMatch(/\.\.\.\(recordLine === "" \? \{\} : \{ recordLine \}\)/);
    const submitEdit = delivery.slice(
      delivery.indexOf("const submitEdit"),
      delivery.indexOf("const flipStatus"),
    );
    expect(submitEdit).toContain("read.body.recordLine");
    // The update body is type-checked against the generated request, so a
    // drifted field name fails compile — but the `satisfies` must stay.
    expect(submitEdit).toContain("satisfies UpdateDomainDnsRecordRequest");
  });

  it("labels the line input and the line column from i18n", () => {
    expect(delivery).toMatch(/aria-label=\{t\("dnsRecordLine"\)\}/);
    expect(delivery).toMatch(/header: t\("dnsRecordLine"\)/);
  });

  it("declares the line label in both locales", () => {
    const i18n = read("packages/sdkwork-deployments-pc-console-delivery/src/i18n.ts");
    expect(i18n).toMatch(/dnsRecordLine: "Line"/);
    expect(i18n).toMatch(/dnsRecordLine: "线路"/);
  });

  it("keeps the ownership entry name distinct from the resolution page", () => {
    // The hostname list previously had a 查看 DNS 记录 action; with the
    // resolution-records page that name collides. The action jumps to the
    // owning record, so it says 归属.
    const i18n = read("packages/sdkwork-deployments-pc-console-delivery/src/i18n.ts");
    expect(i18n).toContain("查看归属记录");
    expect(i18n).toContain("Show ownership record");
  });

  it("mirrors the generated wire contract", () => {
    const updateType = read(
      "../../sdks/sdkwork-deployments-app-sdk/sdkwork-deployments-app-sdk-typescript/generated/server-openapi/src/types/update-domain-dns-record-request.ts",
    );
    expect(updateType).toMatch(/recordLine\?: string/);
    const createType = read(
      "../../sdks/sdkwork-deployments-app-sdk/sdkwork-deployments-app-sdk-typescript/generated/server-openapi/src/types/create-domain-dns-record-request.ts",
    );
    expect(createType).toMatch(/recordLine\?: string/);
  });
});

describe("dns record line field on the admin surface", () => {
  const webserverRoot = resolve(APP_ROOT, "..", "..", "sdkwork-webserver");
  let admin = "";
  try {
    admin = readFileSync(
      resolve(
        webserverRoot,
        "apps/sdkwork-webserver-pc/packages/sdkwork-webserver-pc-admin-delivery/src/ServedDomainAdminSurface.tsx",
      ),
      "utf8",
    );
  } catch {
    // The sibling checkout is optional in CI; the console checks above are
    // the authority there.
  }

  it("carries the same strip field on the admin twin", () => {
    if (!admin) return;
    expect(admin).toMatch(/interface DnsRecordFormValues \{[^}]*recordLine: string/s);
    expect(admin).toMatch(/aria-label=\{t\("resource\.domains\.dnsRecordLine"\)\}/);
    expect(admin).toMatch(/\.\.\.lineFields/);
  });
});
