/**
 * Unit tests for the source-spec routing kernel.
 *
 * This module carries the two wire facts a component gets subtly wrong and then
 * never notices (see the module header): a class's default is its *lowest* rank,
 * and a spec with nothing uploaded produces *no rule at all*. Both failure modes
 * render as a panel that confidently names a source serving nothing, so each
 * behaviour below is asserted against the specific wrong implementation it
 * rules out, not just against "returns something".
 *
 * `planClientClassDefault` gets the strictest oracle: the database enforces
 * `UNIQUE (app_id, environment, client_class, preference)` *per statement*, so
 * the test replays the plan one write at a time and re-checks uniqueness after
 * every statement. A plan that claims rank 0 before releasing it passes a
 * post-state-only check and fails this one — which is the point.
 */
import { describe, expect, it } from "vitest";
import {
  APP_CLIENT_CLASSES,
  CLIENT_ARCHITECTURES,
  CLIENT_ARCHITECTURE_LABEL_KEYS,
  EMPTY_SOURCE_SPEC_DRAFT,
  ROUTE_RANK_OPTIONS,
  ROUTE_UNSERVED,
  RUNTIME_TARGETS,
  RUNTIME_TARGET_LABEL_KEYS,
  SOURCE_SPEC_KEY_PATTERN,
  claimedPreferences,
  describeClientClassRouting,
  describeRoutingOverview,
  findRouteConflict,
  nextFreePreference,
  offeredRanks,
  planClientClassDefault,
  preferenceFor,
  projectSourceSpecLedger,
  sortRoutes,
  specServes,
  splitIndexFiles,
  validateSourceSpecDraft,
  withRoute,
  withoutRoute,
  type SourceSpecLedgerInput,
  type SourceSpecRouteDraft,
  type SourceSpecRoutingInput,
} from "../src/service/app-source-spec-routing.ts";
import type { AppClientClass } from "@sdkwork/deployments-pc-console-core/sdk";

/** Minimal fixture: routing reads six fields, so a spec needs six. */
function spec(overrides: Partial<SourceSpecRoutingInput> & { readonly id: string }): SourceSpecRoutingInput {
  return {
    specKey: overrides.id,
    label: overrides.id.toUpperCase(),
    status: "ACTIVE",
    sourceStatus: "BOUND",
    clientClassRoutes: [],
    ...overrides,
  };
}

function routes(
  ...pairs: readonly (readonly [AppClientClass, number])[]
): readonly SourceSpecRouteDraft[] {
  return pairs.map(([clientClass, preference]) => ({ clientClass, preference }));
}

/** The database's own invariant, replayed statement by statement. */
function expectNoDuplicateRankAfterEachWrite(
  initial: readonly SourceSpecRoutingInput[],
  updates: readonly { readonly specId: string; readonly routes: readonly SourceSpecRouteDraft[] }[],
): void {
  const state = new Map(initial.map((entry) => [entry.id, entry.clientClassRoutes]));
  for (const update of updates) {
    state.set(update.specId, update.routes);
    for (const clientClass of APP_CLIENT_CLASSES) {
      const claimed = [...state.values()]
        .flatMap((list) => list)
        .filter((route) => route.clientClass === clientClass)
        .map((route) => route.preference);
      expect(
        new Set(claimed).size,
        `duplicate rank in ${clientClass} after writing ${update.specId}`,
      ).toBe(claimed.length);
    }
  }
}

/* ------------------------------------------------------------------ *
 * Vocabulary + copy tables
 * ------------------------------------------------------------------ */

describe("contract vocabularies", () => {
  it("keeps the canonical runtime-target count from CONFIG_SPEC §2.1", () => {
    // A locally invented member (or a dropped one) has to fail here: this list
    // is the console's only mirror of the fifteen-value authority.
    expect(RUNTIME_TARGETS).toHaveLength(15);
    expect(Object.keys(RUNTIME_TARGET_LABEL_KEYS)).toHaveLength(RUNTIME_TARGETS.length);
  });

  it("distinguishes PC web from H5 by architecture, not by runtime target", () => {
    // Both are `browser`; `react` vs `react-h5` is the only thing telling them
    // apart. If this ever collapses to one architecture the whole feature is
    // pointless, so assert the discriminator directly.
    expect(RUNTIME_TARGETS).toContain("browser");
    expect(CLIENT_ARCHITECTURE_LABEL_KEYS.react).not.toBe(CLIENT_ARCHITECTURE_LABEL_KEYS["react-h5"]);
    expect(CLIENT_ARCHITECTURES).toHaveLength(Object.keys(CLIENT_ARCHITECTURE_LABEL_KEYS).length);
  });

  it("has copy for every client class the panel renders", () => {
    expect(APP_CLIENT_CLASSES).toHaveLength(6);
  });

  it("pins the spec-key shape the contract owns", () => {
    expect(SOURCE_SPEC_KEY_PATTERN.test("pc.web-1")).toBe(true);
    expect(SOURCE_SPEC_KEY_PATTERN.test("h5:main")).toBe(true);
    expect(SOURCE_SPEC_KEY_PATTERN.test("has space")).toBe(false);
    expect(SOURCE_SPEC_KEY_PATTERN.test("")).toBe(false);
    expect(SOURCE_SPEC_KEY_PATTERN.test("x".repeat(65))).toBe(false);
  });
});

/* ------------------------------------------------------------------ *
 * Route primitives
 * ------------------------------------------------------------------ */

describe("specServes", () => {
  it("requires both ACTIVE and a bound source", () => {
    expect(specServes({ status: "ACTIVE", sourceStatus: "BOUND" })).toBe(true);
    expect(specServes({ status: "DISABLED", sourceStatus: "BOUND" })).toBe(false);
    expect(specServes({ status: "ACTIVE", sourceStatus: "EMPTY" })).toBe(false);
    expect(specServes({ status: "ACTIVE", sourceStatus: "INVALID" })).toBe(false);
    expect(specServes({ status: "ACTIVE", sourceStatus: "REVOKED" })).toBe(false);
  });
});

describe("withRoute / withoutRoute", () => {
  it("replaces a class's rank rather than appending a second entry", () => {
    // Appending would give one spec two ranks in one class, which the drawer
    // would then render as two chain entries for the same source.
    const once = withRoute([], "DESKTOP", 1);
    const twice = withRoute(once, "DESKTOP", 3);
    expect(twice).toEqual([{ clientClass: "DESKTOP", preference: 3 }]);
  });

  it("drops the class when the rank is ROUTE_UNSERVED", () => {
    // The sentinel is the removal signal, so setting it must be indistinguishable
    // from `withoutRoute` — otherwise a select bound to ROUTE_UNSERVED would
    // write a route at rank -1 instead of no route.
    expect(withRoute([{ clientClass: "MOBILE", preference: 0 }], "MOBILE", ROUTE_UNSERVED)).toEqual([]);
    expect(ROUTE_UNSERVED).toBeLessThan(0);
  });

  it("leaves other classes untouched", () => {
    const start = withRoute(withRoute([], "MOBILE", 0), "DESKTOP", 1);
    expect(withoutRoute(start, "MOBILE")).toEqual([{ clientClass: "DESKTOP", preference: 1 }]);
  });
});

describe("sortRoutes", () => {
  it("orders by class first (panel order), then rank", () => {
    const sorted = sortRoutes(routes(["MOBILE", 0], ["DESKTOP", 2], ["DESKTOP", 0]));
    expect(sorted).toEqual([
      { clientClass: "DESKTOP", preference: 0 },
      { clientClass: "DESKTOP", preference: 2 },
      { clientClass: "MOBILE", preference: 0 },
    ]);
  });
});

describe("preferenceFor / claimedPreferences", () => {
  it("reads one class's rank and ignores the rest", () => {
    const list = routes(["MOBILE", 2], ["DESKTOP", 1]);
    expect(preferenceFor(list, "DESKTOP")).toBe(1);
    expect(preferenceFor(list, "TABLET")).toBeUndefined();
  });

  it("collects every rank claimed in a class across specs", () => {
    const specs = [
      spec({ id: "a", clientClassRoutes: routes(["DESKTOP", 0]) }),
      spec({ id: "b", clientClassRoutes: routes(["DESKTOP", 1], ["MOBILE", 0]) }),
    ];
    expect(claimedPreferences(specs, "DESKTOP")).toEqual([0, 1]);
    expect(claimedPreferences(specs, "MOBILE")).toEqual([0]);
  });
});

describe("findRouteConflict", () => {
  it("finds a class claimed twice inside one spec's own list", () => {
    // The DB rejects this across specs; inside one row set it never reaches the
    // DB, so it has to be caught here or it becomes a 500.
    expect(findRouteConflict(routes(["DESKTOP", 1], ["DESKTOP", 1]))).toEqual({
      clientClass: "DESKTOP",
      preference: 1,
    });
  });

  it("returns undefined for a clean list", () => {
    expect(findRouteConflict(routes(["DESKTOP", 0], ["DESKTOP", 1]))).toBeUndefined();
  });
});

describe("nextFreePreference", () => {
  it("starts at 0 when nothing is taken", () => {
    expect(nextFreePreference([])).toBe(0);
  });

  it("skips taken ranks", () => {
    expect(nextFreePreference([0, 1, 3])).toBe(2);
  });

  it("starts at 1 when reserveDefault is set", () => {
    // Rank 0 is the conventional default slot; a caller adding a *fallback*
    // must not silently take it.
    expect(nextFreePreference([], { reserveDefault: true })).toBe(1);
    expect(nextFreePreference([1], { reserveDefault: true })).toBe(2);
  });
});

/* ------------------------------------------------------------------ *
 * Declared vs effective
 * ------------------------------------------------------------------ */

describe("describeClientClassRouting", () => {
  it("names the lowest rank as declared", () => {
    const specs = [
      spec({ id: "pc", clientClassRoutes: routes(["DESKTOP", 0]) }),
      spec({ id: "pc-alt", clientClassRoutes: routes(["DESKTOP", 1]) }),
    ];
    const routing = describeClientClassRouting(specs, "DESKTOP");
    expect(routing.declared?.specId).toBe("pc");
    expect(routing.effective?.specId).toBe("pc");
    expect(routing.fallbackApplied).toBe(false);
    expect(routing.missingDeclaredDefault).toBe(false);
  });

  it("keeps a dark rank in the chain instead of filtering it out", () => {
    // The operator's only clue that the default slot is dark is seeing it. A
    // "filter to serving entries" implementation hides exactly that.
    const specs = [
      spec({ id: "pc", sourceStatus: "EMPTY", clientClassRoutes: routes(["DESKTOP", 0]) }),
      spec({ id: "pc-alt", clientClassRoutes: routes(["DESKTOP", 1]) }),
    ];
    const routing = describeClientClassRouting(specs, "DESKTOP");
    expect(routing.chain.map((entry) => entry.specId)).toEqual(["pc", "pc-alt"]);
    expect(routing.chain[0]?.serves).toBe(false);
  });

  it("falls through to the next rank when the declared default serves nothing", () => {
    // This is the §8 fallback table: no request-time logic, the rule simply is
    // not emitted. `declared` and `effective` therefore differ.
    const specs = [
      spec({ id: "pc", sourceStatus: "EMPTY", clientClassRoutes: routes(["DESKTOP", 0]) }),
      spec({ id: "pc-alt", clientClassRoutes: routes(["DESKTOP", 1]) }),
    ];
    const routing = describeClientClassRouting(specs, "DESKTOP");
    expect(routing.declared?.specId).toBe("pc");
    expect(routing.effective?.specId).toBe("pc-alt");
    expect(routing.fallbackApplied).toBe(true);
  });

  it("does not treat an un-uploaded spec as the effective default", () => {
    // The canonical bug: `preference === 0` read as "the default". Here the
    // class is dark, and nothing must be reported as serving it.
    const specs = [spec({ id: "pc", sourceStatus: "EMPTY", clientClassRoutes: routes(["DESKTOP", 0]) })];
    const routing = describeClientClassRouting(specs, "DESKTOP");
    expect(routing.declared?.specId).toBe("pc");
    expect(routing.effective).toBeUndefined();
    // Nothing falls through: there is no next rank. Dark ≠ falling back.
    expect(routing.fallbackApplied).toBe(false);
  });

  it("treats a DISABLED spec as not serving even with a bound source", () => {
    const specs = [
      spec({ id: "pc", status: "DISABLED", clientClassRoutes: routes(["DESKTOP", 0]) }),
      spec({ id: "pc-alt", clientClassRoutes: routes(["DESKTOP", 1]) }),
    ];
    expect(describeClientClassRouting(specs, "DESKTOP").effective?.specId).toBe("pc-alt");
  });

  it("flags a chain that never claims rank 0", () => {
    // Ranks may be sparse while a set is being built (§8), so rank 1 alone is
    // legal input — it just is not a declared default.
    const specs = [spec({ id: "pc", clientClassRoutes: routes(["DESKTOP", 1]) })];
    const routing = describeClientClassRouting(specs, "DESKTOP");
    expect(routing.declared?.preference).toBe(1);
    expect(routing.missingDeclaredDefault).toBe(true);
    expect(routing.fallbackApplied).toBe(false);
    expect(routing.effective?.specId).toBe("pc");
  });

  it("reports an unrouted class as empty rather than unserved", () => {
    const routing = describeClientClassRouting([], "DESKTOP");
    expect(routing.chain).toEqual([]);
    expect(routing.declared).toBeUndefined();
    expect(routing.effective).toBeUndefined();
    expect(routing.missingDeclaredDefault).toBe(false);
  });

  it("breaks a rank tie by spec key so the panel does not flicker", () => {
    // Two specs at the same rank is a data bug, but rendering it
    // nondeterministically makes it look like a race.
    const specs = [
      spec({ id: "z", specKey: "zzz", clientClassRoutes: routes(["DESKTOP", 0]) }),
      spec({ id: "a", specKey: "aaa", clientClassRoutes: routes(["DESKTOP", 0]) }),
    ];
    expect(describeClientClassRouting(specs, "DESKTOP").declared?.specId).toBe("a");
  });
});

/* ------------------------------------------------------------------ *
 * Overview: the four lists
 * ------------------------------------------------------------------ */

describe("describeRoutingOverview", () => {
  const specs = [
    // DESKTOP: default serves, plus a spare rank.
    spec({ id: "pc", clientClassRoutes: routes(["DESKTOP", 0]) }),
    spec({ id: "pc-alt", clientClassRoutes: routes(["DESKTOP", 1]) }),
    // MOBILE: declared default has nothing uploaded ⇒ falls through.
    spec({ id: "h5", sourceStatus: "EMPTY", clientClassRoutes: routes(["MOBILE", 0]) }),
    spec({ id: "h5-alt", clientClassRoutes: routes(["MOBILE", 1]) }),
    // TABLET: only candidate is dark.
    spec({ id: "tab", sourceStatus: "EMPTY", clientClassRoutes: routes(["TABLET", 0]) }),
    // TV, BOT, OTHER: unrouted.
  ];

  it("classifies each class exactly once along the routing axis", () => {
    const overview = describeRoutingOverview(specs);
    expect(overview.fallingBack).toEqual(["MOBILE"]);
    expect(overview.dark).toEqual(["TABLET"]);
    expect(overview.unrouted).toEqual(["TV", "BOT", "OTHER"]);
    expect(overview.missingDeclaredDefault).toEqual([]);
  });

  it("keeps unrouted and routed classes disjoint and exhaustive", () => {
    const overview = describeRoutingOverview(specs);
    expect(overview.routedClassCount).toBe(APP_CLIENT_CLASSES.length - overview.unrouted.length);
    expect(overview.routedClassCount).toBe(3);
  });

  it("counts effective defaults as routed minus dark", () => {
    // Recomputing these separately is how they drift; this asserts the identity
    // the single walk guarantees.
    const overview = describeRoutingOverview(specs);
    expect(overview.effectiveDefaultCount).toBe(overview.routedClassCount - overview.dark.length);
    expect(overview.effectiveDefaultCount).toBe(2);
    // Three classes *declare* a default, only two have one that serves — which
    // is the whole reason the panel shows both numbers.
    expect(overview.declaredDefaultCount).toBe(3);
    expect(overview.declaredDefaultCount).toBeLessThanOrEqual(overview.routedClassCount);
  });

  it("never reports a class as both falling back and dark", () => {
    // A class cannot have an effective entry (falling back) and no effective
    // entry (dark) at once — the failure mode of four independent derivations.
    const overview = describeRoutingOverview(specs);
    const overlap = overview.fallingBack.filter((clientClass) => overview.dark.includes(clientClass));
    expect(overlap).toEqual([]);
  });

  it("reports a class that is both undeclared-default and falling back", () => {
    // Sparse ranks make this combination reachable: candidates exist, none at
    // rank 0, and the lowest one is dark.
    const sparse = [
      spec({ id: "a", sourceStatus: "EMPTY", clientClassRoutes: routes(["DESKTOP", 1]) }),
      spec({ id: "b", clientClassRoutes: routes(["DESKTOP", 2]) }),
    ];
    const overview = describeRoutingOverview(sparse);
    expect(overview.missingDeclaredDefault).toEqual(["DESKTOP"]);
    expect(overview.fallingBack).toEqual(["DESKTOP"]);
  });
});

/* ------------------------------------------------------------------ *
 * Re-pointing a default
 * ------------------------------------------------------------------ */

describe("planClientClassDefault", () => {
  const specs = [
    { ...spec({ id: "pc", clientClassRoutes: routes(["DESKTOP", 0]) }), version: "v1" },
    { ...spec({ id: "pc-alt", clientClassRoutes: routes(["DESKTOP", 1]) }), version: "v2" },
    { ...spec({ id: "unrouted" }), version: "v3" },
  ];

  it("releases rank 0 before claiming it", () => {
    // The single most important assertion in this file: a plan that claims
    // first produces two rank-0 DESKTOP rows and the DB rejects it.
    const plan = planClientClassDefault(specs, "DESKTOP", "pc-alt");
    expect(plan.unchanged).toBe(false);
    expect(plan.updates[0]?.specId).toBe("pc");
    expect(plan.updates[1]?.specId).toBe("pc-alt");
    expectNoDuplicateRankAfterEachWrite(specs, plan.updates);
  });

  it("demotes the outgoing default instead of deleting its route", () => {
    // Dropping it would leave the class dark while the new default is still
    // EMPTY — the exact gap the fallback chain exists to cover.
    const plan = planClientClassDefault(specs, "DESKTOP", "pc-alt");
    const released = plan.updates[0];
    expect(preferenceFor(released?.routes ?? [], "DESKTOP")).toBe(2);
    expect(plan.demotedSpecId).toBe("pc");
  });

  it("skips the rank the target is about to vacate", () => {
    // Regression: the holder may not move into rank 1 here, because statement 1
    // runs while `pc-alt` still occupies it. Planning rank 1 makes the most
    // common re-point ("pc at 0, pc-alt at 1, swap them") fail on the DB's
    // per-statement unique check. Rank 1 is therefore left empty until the
    // operator re-points again — a legal sparse chain, and the only plan that
    // cannot be rejected.
    const plan = planClientClassDefault(specs, "DESKTOP", "pc-alt");
    const released = plan.updates[0];
    expect(plan.updates[0]?.specId).toBe("pc");
    expect(preferenceFor(released?.routes ?? [], "DESKTOP")).not.toBe(1);
    expectNoDuplicateRankAfterEachWrite(specs, plan.updates);
  });

  it("moves the incoming default to rank 0", () => {
    const plan = planClientClassDefault(specs, "DESKTOP", "pc-alt");
    expect(preferenceFor(plan.updates[1]?.routes ?? [], "DESKTOP")).toBe(0);
  });

  it("carries the version each write must match", () => {
    const plan = planClientClassDefault(specs, "DESKTOP", "pc-alt");
    expect(plan.updates.map((update) => update.version)).toEqual(["v1", "v2"]);
  });

  it("is a no-op when the target already holds rank 0", () => {
    const plan = planClientClassDefault(specs, "DESKTOP", "pc");
    expect(plan.unchanged).toBe(true);
    expect(plan.updates).toEqual([]);
    expect(plan.demotedSpecId).toBeUndefined();
  });

  it("is a no-op for an unknown target", () => {
    expect(planClientClassDefault(specs, "DESKTOP", "nope")).toEqual({ updates: [], unchanged: true });
  });

  it("promotes a spec that serves the class at no rank yet", () => {
    // `unrouted` declares nothing for DESKTOP; making it the default must add
    // rank 0 *and* still move the holder out of the way.
    const plan = planClientClassDefault(specs, "DESKTOP", "unrouted");
    expect(plan.updates).toHaveLength(2);
    expect(preferenceFor(plan.updates[1]?.routes ?? [], "DESKTOP")).toBe(0);
    expectNoDuplicateRankAfterEachWrite(specs, plan.updates);
  });

  it("demotes into the lowest rank nothing will occupy", () => {
    // Ranks 0, 1 and 2 are all held when statement 1 runs (rank 2 by the very
    // spec being promoted), so the holder has to land on 3.
    const crowded = [
      { ...spec({ id: "pc", clientClassRoutes: routes(["DESKTOP", 0]) }), version: "v1" },
      { ...spec({ id: "mid", clientClassRoutes: routes(["DESKTOP", 1]) }), version: "v2" },
      { ...spec({ id: "next", clientClassRoutes: routes(["DESKTOP", 2]) }), version: "v3" },
    ];
    const plan = planClientClassDefault(crowded, "DESKTOP", "next");
    expect(plan.updates[0]?.specId).toBe("pc");
    expect(preferenceFor(plan.updates[0]?.routes ?? [], "DESKTOP")).toBe(3);
    expectNoDuplicateRankAfterEachWrite(crowded, plan.updates);
  });

  it("re-compacts the chain when the default is pointed back", () => {
    // The gap the previous test leaves is not permanent: pointing the default
    // the other way slides the other spec into the freed rank.
    const afterSwap = [
      { ...spec({ id: "pc", clientClassRoutes: routes(["DESKTOP", 2]) }), version: "v1" },
      { ...spec({ id: "pc-alt", clientClassRoutes: routes(["DESKTOP", 0]) }), version: "v2" },
    ];
    const plan = planClientClassDefault(afterSwap, "DESKTOP", "pc");
    expect(preferenceFor(plan.updates[0]?.routes ?? [], "DESKTOP")).toBe(1);
    expect(preferenceFor(plan.updates[1]?.routes ?? [], "DESKTOP")).toBe(0);
    expectNoDuplicateRankAfterEachWrite(afterSwap, plan.updates);
  });

  it("only touches routes for the class being re-pointed", () => {
    const mixed = [
      { ...spec({ id: "pc", clientClassRoutes: routes(["DESKTOP", 0], ["MOBILE", 0]) }), version: "v1" },
      { ...spec({ id: "pc-alt", clientClassRoutes: routes(["DESKTOP", 1]) }), version: "v2" },
    ];
    const plan = planClientClassDefault(mixed, "DESKTOP", "pc-alt");
    const released = plan.updates[0];
    expect(preferenceFor(released?.routes ?? [], "MOBILE")).toBe(0);
  });
});

/* ------------------------------------------------------------------ *
 * offeredRanks
 * ------------------------------------------------------------------ */

describe("offeredRanks", () => {
  it("offers the whole closed rank list when nothing is claimed", () => {
    expect(offeredRanks({ id: "a", clientClassRoutes: [] }, "DESKTOP", [])).toEqual([...ROUTE_RANK_OPTIONS]);
  });

  it("hides ranks a sibling already holds", () => {
    const sibling = spec({ id: "b", clientClassRoutes: routes(["DESKTOP", 0]) });
    expect(offeredRanks({ id: "a", clientClassRoutes: [] }, "DESKTOP", [sibling])).toEqual([1, 2, 3]);
  });

  it("keeps a rank the spec already holds even when it is out of the closed list", () => {
    // A spec set to rank 7 by an older client must still render as rank 7.
    // Snapping it to "not served" would silently delete the route on save —
    // `withRoute(…, ROUTE_UNSERVED)` drops the class entirely.
    const owner = spec({ id: "a", clientClassRoutes: routes(["DESKTOP", 7]) });
    const sibling = spec({
      id: "b",
      clientClassRoutes: routes(["DESKTOP", 0], ["DESKTOP", 1], ["DESKTOP", 2], ["DESKTOP", 3]),
    });
    const offered = offeredRanks(owner, "DESKTOP", [owner, sibling]);
    // Nothing in the closed list is free, and 7 is still offered, so the select
    // shows the spec's real rank instead of an empty option.
    expect(offered).toEqual([7]);
  });

  it("appends the spec's own out-of-band rank rather than replacing the list", () => {
    // Same claim, with room left in the closed list: the console must offer both
    // "re-rank into the normal range" and "leave it as it is".
    const owner = spec({ id: "a", clientClassRoutes: routes(["DESKTOP", 7]) });
    const sibling = spec({ id: "b", clientClassRoutes: routes(["DESKTOP", 0]) });
    expect(offeredRanks(owner, "DESKTOP", [owner, sibling])).toEqual([1, 2, 3, 7]);
  });

  it("does not treat the spec's own rank as taken by a sibling", () => {
    const owner = spec({ id: "a", clientClassRoutes: routes(["DESKTOP", 1]) });
    expect(offeredRanks(owner, "DESKTOP", [owner])).toContain(1);
  });
});

/* ------------------------------------------------------------------ *
 * Draft validation
 * ------------------------------------------------------------------ */

describe("validateSourceSpecDraft", () => {
  const draft = { ...EMPTY_SOURCE_SPEC_DRAFT, specKey: "pc.web", label: "PC web" };

  it("accepts the default draft once key and label are filled", () => {
    expect(validateSourceSpecDraft(draft, [])).toEqual([]);
  });

  it("requires a key and a label", () => {
    expect(validateSourceSpecDraft({ ...draft, specKey: "" }, [])).toContain("specKeyRequired");
    expect(validateSourceSpecDraft({ ...draft, label: "   " }, [])).toContain("labelRequired");
  });

  it("rejects a key outside the contract's shape", () => {
    expect(validateSourceSpecDraft({ ...draft, specKey: "not a key" }, [])).toContain("specKeyInvalid");
  });

  it("rejects a key a sibling already uses", () => {
    expect(validateSourceSpecDraft(draft, [{ id: "other", specKey: "pc.web" }])).toContain("specKeyTaken");
  });

  it("trims before comparing keys", () => {
    expect(validateSourceSpecDraft({ ...draft, specKey: "  pc.web  " }, [{ id: "other", specKey: "pc.web" }]))
      .toContain("specKeyTaken");
  });

  it("rejects the same (class, rank) twice", () => {
    // This is the pair the database's unique index would reject, which is why it
    // is a field error rather than a round trip.
    const conflicted = { ...draft, routes: routes(["DESKTOP", 0], ["DESKTOP", 0]) };
    expect(validateSourceSpecDraft(conflicted, [])).toContain("routeDuplicate");
  });

  it("accepts one class listed at two different ranks", () => {
    // Deliberately not an error. The database keys on
    // `(app, env, class, preference)`, so this row set is accepted; refusing it
    // here would make the form stricter than the server. The redundancy is
    // prevented at the source instead — every edit goes through `withRoute`,
    // which replaces rather than appends.
    const redundant = { ...draft, routes: routes(["DESKTOP", 0], ["DESKTOP", 1]) };
    expect(validateSourceSpecDraft(redundant, [])).not.toContain("routeDuplicate");
  });

  it("rejects a fractional or negative rank", () => {
    expect(validateSourceSpecDraft({ ...draft, routes: routes(["DESKTOP", -1]) }, [])).toContain("routeRankNegative");
    expect(validateSourceSpecDraft({ ...draft, routes: routes(["DESKTOP", 1.5]) }, [])).toContain("routeRankNegative");
    expect(validateSourceSpecDraft({ ...draft, routes: routes(["DESKTOP", 0]) }, [])).not.toContain("routeRankNegative");
  });

  it("de-duplicates issues so one message is rendered per problem", () => {
    const doubled = { ...draft, routes: routes(["DESKTOP", -1], ["DESKTOP", -1]) };
    const issues = validateSourceSpecDraft(doubled, []);
    expect(issues.filter((issue) => issue === "routeRankNegative")).toHaveLength(1);
  });
});

/* ------------------------------------------------------------------ *
 * Form ↔ wire
 * ------------------------------------------------------------------ */

describe("splitIndexFiles", () => {
  it("splits on commas and whitespace", () => {
    expect(splitIndexFiles("index.html, index.htm")).toEqual(["index.html", "index.htm"]);
    expect(splitIndexFiles("index.html  index.htm")).toEqual(["index.html", "index.htm"]);
  });

  it("drops empty entries", () => {
    expect(splitIndexFiles(" , , ")).toEqual([]);
    expect(splitIndexFiles("")).toEqual([]);
  });
});

/* ------------------------------------------------------------------ *
 * Cross-application ledger
 * ------------------------------------------------------------------ */

describe("projectSourceSpecLedger", () => {
  function ledgerSpec(
    overrides: Partial<SourceSpecLedgerInput> & { readonly id: string },
  ): SourceSpecLedgerInput {
    return {
      specKey: overrides.id,
      label: overrides.id.toUpperCase(),
      status: "ACTIVE",
      sourceStatus: "BOUND",
      clientClassRoutes: [],
      runtimeTarget: "browser",
      clientArchitecture: "react",
      handler: "SPA",
      isDefault: false,
      ...overrides,
    };
  }

  it("marks the declared default and the rank that actually answers, separately", () => {
    // The canonical dark-default case: PC is declared the Desktop default but has
    // nothing uploaded, so H5 answers. The ledger has to say both — marking the
    // rank-0 row as "effective" would name a source that serves nothing.
    const ledger = projectSourceSpecLedger([
      ledgerSpec({
        id: "pc",
        status: "ACTIVE",
        sourceStatus: "EMPTY",
        clientClassRoutes: [{ clientClass: "DESKTOP", preference: 0 }],
      }),
      ledgerSpec({ id: "h5", clientClassRoutes: [{ clientClass: "DESKTOP", preference: 1 }] }),
    ]);

    expect(ledger[0]?.routes).toEqual([
      { clientClass: "DESKTOP", preference: 0, isDeclaredDefault: true, serves: false, isEffective: false },
    ]);
    expect(ledger[1]?.routes).toEqual([
      { clientClass: "DESKTOP", preference: 1, isDeclaredDefault: false, serves: true, isEffective: true },
    ]);
  });

  it("carries the ledger-only fields through untouched", () => {
    // The admin surface shows runtime target / architecture / handler and the
    // app-level fallback flag, none of which routing reads. A projection that
    // dropped them would render blank columns rather than fail.
    const ledger = projectSourceSpecLedger([
      ledgerSpec({
        id: "h5",
        label: "H5",
        runtimeTarget: "browser",
        clientArchitecture: "react-h5",
        handler: "SPA",
        isDefault: true,
      }),
    ]);
    expect(ledger[0]).toMatchObject({
      specId: "h5",
      specKey: "h5",
      label: "H5",
      clientArchitecture: "react-h5",
      handler: "SPA",
      isAppDefault: true,
    });
  });

  it("marks at most one route per class as the effective one", () => {
    // Two ranks of the same class inside one spec are accepted by the server
    // (redundant, not illegal). Keying "effective" by spec rather than by
    // `spec#rank` would light up both rows and make the ledger claim the same
    // class answers twice.
    const ledger = projectSourceSpecLedger([
      ledgerSpec({
        id: "pc",
        clientClassRoutes: [
          { clientClass: "DESKTOP", preference: 0 },
          { clientClass: "DESKTOP", preference: 2 },
        ],
      }),
    ]);
    expect(ledger[0]?.routes.filter((route) => route.isEffective)).toHaveLength(1);
    expect(ledger[0]?.routes.find((route) => route.isEffective)?.preference).toBe(0);
  });

  it("leaves isEffective unset for a class that nothing serves", () => {
    const ledger = projectSourceSpecLedger([
      ledgerSpec({
        id: "pc",
        sourceStatus: "EMPTY",
        clientClassRoutes: [{ clientClass: "MOBILE", preference: 0 }],
      }),
    ]);
    expect(ledger[0]?.routes[0]).toMatchObject({ serves: false, isEffective: false });
  });
});
