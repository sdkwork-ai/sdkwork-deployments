import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";

import { describe, expect, it } from "vitest";

// A component-level early return that sits above a hook makes the hook count a
// function of the state being tested, and React answers that with
// "Rendered fewer hooks than expected" — the whole subtree unmounts. The crash
// is state-dependent, so it survives a plain render test: `DomainHostnameList`
// reached this exact shape through its loading guard, which only short-circuits
// on the *second* render (the one its own effect schedules), so the first
// render looked healthy.
//
// The rule is therefore checked on the source rather than through a render:
// within one component, no `use*` call may appear after a top-level
// `if (...) return`. Indentation is not the signal — a component-level return
// in this codebase is a statement at brace depth one — so the depth is tracked
// rather than pattern-matched, which is what let the original defect through a
// plain grep.

const SOURCE = resolve(
  dirname(fileURLToPath(import.meta.url)),
  "../packages/sdkwork-deployments-pc-console-delivery/src/DeliveryManagement.tsx",
);

const HOOK_CALL =
  /(?:React\.)?\b(?:useState|useEffect|useLayoutEffect|useInsertionEffect|useMemo|useCallback|useRef|useImperativeHandle|useReducer|useContext|useId|useSyncExternalStore|useTransition|useDeferredValue)\s*[<(]/;

interface Finding {
  component: string;
  returnLine: number;
  hookLine: number;
  hookText: string;
}

/**
 * Component-level early returns followed by a hook, in the same component.
 *
 * Walks the file once, tracking brace depth so that a `return` inside a
 * callback (`.then((result) => { if (!active) return; ... })`) is not mistaken
 * for one that short-circuits the render, and so that a hook belonging to an
 * inner component is not attributed to the outer one.
 */
export function findHooksAfterEarlyReturn(source: string): Finding[] {
  const lines = source.split(/\r?\n/);
  const findings: Finding[] = [];

  let depth = 0;
  let component: string | null = null;
  let pendingReturn: { line: number; component: string } | null = null;

  for (let index = 0; index < lines.length; index += 1) {
    const line = lines[index]!.replace(/\/\/.*$/, "");

    if (depth === 0) {
      const declared = /^\s*(?:export\s+)?(?:default\s+)?function\s+([A-Z][A-Za-z0-9_]*)/.exec(line);
      if (declared) {
        component = declared[1]!;
        pendingReturn = null;
      }
    }

    // At depth 1 the statement belongs to the component body itself.
    if (depth === 1 && component !== null) {
      if (/^ {2}if\s*\(.+\)\s*return\b/.test(line)) {
        pendingReturn = { line: index + 1, component };
      } else if (pendingReturn && HOOK_CALL.test(line)) {
        findings.push({
          component: pendingReturn.component,
          returnLine: pendingReturn.line,
          hookLine: index + 1,
          hookText: line.trim(),
        });
      }
    }

    for (const character of line) {
      if (character === "{" || character === "(" || character === "[") depth += 1;
      else if (character === "}" || character === ")" || character === "]") depth -= 1;
      if (depth < 0) depth = 0;
    }

    if (depth === 0) {
      component = null;
      pendingReturn = null;
    }
  }

  return findings;
}

describe("console delivery hook order", () => {
  it("keeps every hook above the component's own early returns", () => {
    const findings = findHooksAfterEarlyReturn(readFileSync(SOURCE, "utf8"));

    const rendered = findings
      .map(
        (finding) =>
          `${finding.component}: return at line ${finding.returnLine} precedes the hook at line ${finding.hookLine} (${finding.hookText})`,
      )
      .join("\n");

    expect(rendered).toBe("");
  });

  // The detector is only worth its green when it can go red. A guard above a
  // hook has to be reported; the same guard placed after the hook must not be.
  it("reports a guard that sits above a hook", () => {
    const broken = [
      "function Widget() {",
      "  const [busy, setBusy] = useState(false);",
      "  if (!ready && busy) return <span>loading</span>;",
      "  const columns = useMemo(() => [], []);",
      "  return <table />;",
      "}",
    ].join("\n");

    expect(findHooksAfterEarlyReturn(broken)).toEqual([
      { component: "Widget", returnLine: 3, hookLine: 4, hookText: "const columns = useMemo(() => [], []);" },
    ]);
  });

  it("does not flag a guard placed below the hooks", () => {
    const fixed = [
      "function Widget() {",
      "  const [busy, setBusy] = useState(false);",
      "  const columns = useMemo(() => [], []);",
      "  if (!ready && busy) return <span>loading</span>;",
      "  return <table />;",
      "}",
    ].join("\n");

    expect(findHooksAfterEarlyReturn(fixed)).toEqual([]);
  });

  it("does not flag a callback's own early return", () => {
    const callbackReturn = [
      "function Widget() {",
      "  useEffect(() => {",
      "    void load().then((result) => {",
      "      if (!active) return;",
      "      setRows(result);",
      "    });",
      "  }, []);",
      "  const columns = useMemo(() => [], []);",
      "  return <table />;",
      "}",
    ].join("\n");

    expect(findHooksAfterEarlyReturn(callbackReturn)).toEqual([]);
  });
});
