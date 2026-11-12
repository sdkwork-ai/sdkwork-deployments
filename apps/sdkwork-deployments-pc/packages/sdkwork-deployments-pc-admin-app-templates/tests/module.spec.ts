import { describe, expect, it } from "vitest";

import { deploymentsModule } from "../src/module.ts";

/**
 * Guards the app-center contract the shell depends on: four resources in the
 * operator's reading order, all gated, on the backend-admin surface. A resource
 * added here without a matching `DeploymentsResourceKey`/i18n entry would
 * otherwise fail only at runtime with a blank route.
 */
describe("app center module", () => {
  it("contributes the applications ledger before the template catalog", () => {
    expect(deploymentsModule.entries.map((entry) => entry.resource)).toEqual([
      "applications",
      "appTemplates",
      "templateCategories",
      "appTemplateVersions",
    ]);
    expect(deploymentsModule.entries.map((entry) => entry.order)).toEqual([1, 2, 3, 4]);
  });

  it("gates every entry and stays on the backend-admin surface", () => {
    expect(deploymentsModule.surface).toBe("backend-admin");
    expect(deploymentsModule.id).toBe("app-center");
    expect(deploymentsModule.entries.every((entry) => Boolean(entry.permission))).toBe(true);
  });
});
