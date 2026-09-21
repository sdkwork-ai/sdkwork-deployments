import { readFileSync, readdirSync, statSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

const root = resolve(import.meta.dirname, "..");

function files(directory: string): string[] {
  return readdirSync(directory)
    .filter((name) => name !== "node_modules")
    .flatMap((name) => {
      const path = resolve(directory, name);
      return statSync(path).isDirectory() ? files(path) : /\.tsx?$/.test(path) ? [path] : [];
    });
}

// Authored table markup is read from code, not from prose: a comment that merely
// names the tag (the workspace comment explaining why a `DataTable` replaced one)
// is documentation, and matching it would fail the gate for documenting the rule
// it enforces. Block comments are blanked first, then line comments.
function authoredTable(source: string): boolean {
  const withoutBlockComments = source.replace(/\/\*[\s\S]*?\*\//g, "");
  const withoutLineComments = withoutBlockComments.replace(/^\s*\/\/[^\n]*$/gm, "");
  return /<table[\s>]/.test(withoutLineComments);
}

describe("deployment surface boundaries", () => {
  it("keeps backend SDK out of console", () =>
    expect(
      files(resolve(root, "packages")).filter(
        (path) => path.includes("-console-") && readFileSync(path, "utf8").includes("@sdkwork/deployments-backend-sdk"),
      ),
    ).toEqual([]));

  it("keeps app and Drive SDKs out of admin", () =>
    expect(
      files(resolve(root, "packages")).filter(
        (path) => path.includes("-admin-") && /@sdkwork\/(deploy-app-sdk|drive-app-sdk)/.test(readFileSync(path, "utf8")),
      ),
    ).toEqual([]));

  it("contains no authored raw HTTP", () =>
    expect(files(resolve(root, "packages")).filter((path) => /\bfetch\s*\(/.test(readFileSync(path, "utf8")))).toEqual([]));

  // Console and admin tables are rendered by the framework `DataTable`, which owns
  // pagination, sorting, sticky headers, selection, and row actions
  // (`framework-governance.md`: dense table chrome belongs on the composite, not on
  // hand-rolled markup). This repository has no content-only table, so authored
  // table markup must not appear at all.
  it("renders data tables through the framework DataTable", () =>
    expect(files(resolve(root, "packages")).filter((path) => authoredTable(readFileSync(path, "utf8")))).toEqual([]));
});
