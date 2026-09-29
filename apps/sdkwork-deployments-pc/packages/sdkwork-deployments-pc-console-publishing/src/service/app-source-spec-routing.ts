/**
 * Source specs and client-class routing — the console's model of "which source
 * serves which kind of client, and in what order".
 *
 * ## Why this is its own module
 *
 * `deploy_app_source_spec` answers "how many sources does this app have". The
 * question an operator actually asks is "when a phone requests this app, which
 * of them answers — and if that one has no uploaded source, which answers
 * instead". Two wire facts decide it, and neither is a field:
 *
 * 1. **A client class's default is its lowest-ranked spec.** The contract stores
 *    an ordered `preference`, not a flag, and ranks may be sparse while a set is
 *    being built — `SDKWORK_DEPLOY_SPEC.md` §8 lets a class be declared at rank 1
 *    before any spec claims rank 0. So `preference === 0` is the *convention* for
 *    "this is the default", not the definition: the definition is "nobody else in
 *    this class ranks lower".
 * 2. **A spec with no uploaded source produces no routing rule at all**, which is
 *    how the §8 fallback table is implemented without request-time logic. A
 *    rank-0 spec still sitting at `EMPTY` therefore serves nothing, and the
 *    request lands on the next rank on its own. The rank the operator wrote and
 *    the rank that actually answers are two different things, and only the second
 *    one is what a user experiences.
 *
 * Both facts are the kind that a component gets subtly wrong (`preference === 0`
 * reads like "the default" right up until the un-uploaded case appears), and
 * getting them wrong is invisible: the panel would confidently name a spec that
 * serves nothing. They therefore live here as pure functions, and the drawer only
 * renders what they return.
 *
 * The functions are deliberately **structural** — they take the slice of
 * `AppSourceSpecResponse` they read, so a fixture needs no `id`/`version`/
 * timestamps, and the module stays testable without a renderer.
 *
 * `planClientClassDefault` is the one with real ordering risk: re-pointing a
 * class's default is a **two-row** write, and the database enforces
 * `UNIQUE (app_id, environment, client_class, preference)`, so a naive
 * "add the new one first" leaves a transient duplicate and is rejected. The plan
 * it returns is ordered, and it demotes rather than drops the outgoing default so
 * the fallback chain keeps serving while the new default is still `EMPTY`.
 */
import type {
  AppClientArchitecture,
  AppClientClass,
  AppPublishEnvironment,
  AppSourceBindingStatus,
  AppSourceSpecStatus,
  SdkworkRuntimeTarget,
} from "@sdkwork/deployments-pc-console-core/sdk";
import type { PublishingMessageKey } from "../i18n.ts";

/* ------------------------------------------------------------------ *
 * Environment
 * ------------------------------------------------------------------ */

/**
 * The contract's `AppPublishEnvironment` members, in display order.
 *
 * `satisfies` rather than a partial map: the spec's `environment` column carries
 * the same CHECK, so a member added to the contract is a compile error here
 * until this list grows with it — **and so is a member that is not there**. That
 * second half is not hypothetical: this list previously read
 * `development/test/staging/demo/production` under a comment claiming it matched
 * the contract, and the fifth entry was simply wrong. The spec's `environment`
 * column CHECK has no `demo`; the drawer was offering a value every create
 * request would have been rejected for. The write path could not fail loudly
 * because the list was typed `readonly string[]` at the point of use, so nothing
 * compared it to the contract at all.
 *
 * ⚠️ **Two environment vocabularies, and they are not the same set.**
 * `AppPublishEnvironment` here is the **publish** axis — where an app is
 * published, which is what a source spec is scoped by. `DeployEnvironmentId`
 * (see `project-detection.ts`) and `DeploymentsEnvironment` (`pc-core`) are the
 * **build** axis and do carry `demo`. Do not "unify" them: a build can target
 * `demo` while no publish environment by that name exists, and swapping one list
 * for the other is how the defect above was introduced.
 */
export const APP_PUBLISH_ENVIRONMENTS = [
  "development",
  "test",
  "staging",
  "production",
] as const satisfies readonly AppPublishEnvironment[];

/**
 * The environment a surface should open on for one app.
 *
 * `app.defaultEnvironment` is the *app-level* publishing default and is **not**
 * guaranteed to be a member of {@link APP_PUBLISH_ENVIRONMENTS}: the column is
 * free-form, and legacy rows can hold an empty string or a value another surface
 * wrote. Sending it straight back as a `listSourceSpecs` query would ask for a
 * set that cannot exist, so an unrecognised value falls back to `production`
 * instead of being trusted.
 *
 * Structural input on purpose — both the drawer and the source dialog need this
 * and neither should have to pass a whole `AppResponse`.
 */
export function resolveAppEnvironment(
  app: { readonly defaultEnvironment?: string | undefined },
): AppPublishEnvironment {
  const declared = app.defaultEnvironment
  return (APP_PUBLISH_ENVIRONMENTS as readonly string[]).includes(declared ?? "")
    ? declared as AppPublishEnvironment
    : "production";
}

/* ------------------------------------------------------------------ *
 * Contract vocabularies, in display order
 * ------------------------------------------------------------------ */

/**
 * Client classes in the order the console shows them.
 *
 * Sourced from the contract's `AppClientClass`; the `satisfies` keeps the list
 * honest when the enum grows (a new member is a compile error here, not a class
 * that silently never appears in the panel).
 */
export const APP_CLIENT_CLASSES = [
  "DESKTOP",
  "MOBILE",
  "TABLET",
  "TV",
  "BOT",
  "OTHER",
] as const satisfies readonly AppClientClass[];

/**
 * `runtimeTarget` in the order that puts the web pair first.
 *
 * The order is display-only; the members are the canonical fifteen from
 * `CONFIG_SPEC.md` §2.1 and are never extended locally.
 */
export const RUNTIME_TARGETS = [
  "browser",
  "desktop",
  "tablet-ipados",
  "tablet-android",
  "capacitor-ios",
  "capacitor-android",
  "flutter-ios",
  "flutter-android",
  "android-native",
  "ios-native",
  "harmony-native",
  "mini-program",
  "server",
  "container",
  "test-runner",
] as const satisfies readonly SdkworkRuntimeTarget[];

/** Architectures within a runtime target, web pair first. */
export const CLIENT_ARCHITECTURES = [
  "react",
  "react-h5",
  "static-web",
  "flutter-mobile",
  "mini-program",
  "android-mobile",
  "ios-mobile",
  "harmony-mobile",
  "unity",
  "uniapp",
  "pad",
] as const satisfies readonly AppClientArchitecture[];

/**
 * `specKey` is the contract's `CompositionKey`, and the contract is the only
 * authority for its shape: 1–64 characters from `[A-Za-z0-9._:-]`. Checked here
 * so the form fails before a round trip, not because this module owns the rule.
 */
export const SOURCE_SPEC_KEY_PATTERN = /^[A-Za-z0-9._:-]{1,64}$/;

/* ------------------------------------------------------------------ *
 * Enum → copy
 * ------------------------------------------------------------------ *
 * `Record<Union, …>` rather than a partial map: adding a member to any of these
 * contract vocabularies becomes a compile error here until it has copy, which is
 * what stops a new runtime target from being rendered as a raw token.
 */

export const CLIENT_CLASS_LABEL_KEYS: Readonly<Record<AppClientClass, PublishingMessageKey>> = {
  DESKTOP: "clientClassDesktop",
  MOBILE: "clientClassMobile",
  TABLET: "clientClassTablet",
  TV: "clientClassTv",
  BOT: "clientClassBot",
  OTHER: "clientClassOther",
};

export const RUNTIME_TARGET_LABEL_KEYS: Readonly<Record<SdkworkRuntimeTarget, PublishingMessageKey>> = {
  browser: "rtBrowser",
  desktop: "rtDesktop",
  "tablet-ipados": "rtTabletIpadOs",
  "tablet-android": "rtTabletAndroid",
  "capacitor-ios": "rtCapacitorIos",
  "capacitor-android": "rtCapacitorAndroid",
  "flutter-ios": "rtFlutterIos",
  "flutter-android": "rtFlutterAndroid",
  "android-native": "rtAndroidNative",
  "ios-native": "rtIosNative",
  "harmony-native": "rtHarmonyNative",
  "mini-program": "rtMiniProgram",
  server: "rtServer",
  container: "rtContainer",
  "test-runner": "rtTestRunner",
};

export const CLIENT_ARCHITECTURE_LABEL_KEYS: Readonly<Record<AppClientArchitecture, PublishingMessageKey>> = {
  react: "archReact",
  "react-h5": "archReactH5",
  "static-web": "archStaticWeb",
  "flutter-mobile": "archFlutterMobile",
  "mini-program": "archMiniProgram",
  "android-mobile": "archAndroidMobile",
  "ios-mobile": "archIosMobile",
  "harmony-mobile": "archHarmonyMobile",
  unity: "archUnity",
  uniapp: "archUniapp",
  pad: "archPad",
};

export const SOURCE_BINDING_LABEL_KEYS: Readonly<Record<AppSourceBindingStatus, PublishingMessageKey>> = {
  EMPTY: "sourceBindingEmpty",
  BOUND: "sourceBindingBound",
  INVALID: "sourceBindingInvalid",
  REVOKED: "sourceBindingRevoked",
};

export const SOURCE_SPEC_STATUS_LABEL_KEYS: Readonly<Record<AppSourceSpecStatus, PublishingMessageKey>> = {
  ACTIVE: "specStatusActive",
  DISABLED: "specStatusDisabled",
};

export const SOURCE_SPEC_HANDLER_LABEL_KEYS: Readonly<Record<"STATIC" | "SPA" | "WIKI", PublishingMessageKey>> = {
  STATIC: "handlerStatic",
  SPA: "handlerSpa",
  WIKI: "handlerWiki",
};

/* ------------------------------------------------------------------ *
 * Route drafts
 * ------------------------------------------------------------------ */

/** One `(clientClass, preference)` pair — the wire shape of a route. */
export interface SourceSpecRouteDraft {
  readonly clientClass: AppClientClass
  readonly preference: number
}

/**
 * The slice of a source spec that routing depends on.
 *
 * `AppSourceSpecResponse` satisfies this structurally, so responses are passed
 * straight through and tests build fixtures from four fields.
 */
export interface SourceSpecRoutingInput {
  readonly id: string
  readonly specKey: string
  readonly label: string
  readonly status: AppSourceSpecStatus
  readonly sourceStatus: AppSourceBindingStatus
  readonly clientClassRoutes: readonly SourceSpecRouteDraft[]
}

/** Rank a class at which nothing is declared for it. */
export const ROUTE_UNSERVED = -1

function classRank(clientClass: AppClientClass): number {
  const index = APP_CLIENT_CLASSES.indexOf(clientClass);
  // An unknown class sorts last rather than throwing: a server that grew the
  // vocabulary should render, not blank the panel.
  return index === -1 ? APP_CLIENT_CLASSES.length : index;
}

/** Deterministic order: class, then rank, then key. */
export function compareRoutes(
  left: SourceSpecRouteDraft,
  right: SourceSpecRouteDraft,
): number {
  const byClass = classRank(left.clientClass) - classRank(right.clientClass);
  return byClass !== 0 ? byClass : left.preference - right.preference;
}

export function sortRoutes(routes: readonly SourceSpecRouteDraft[]): readonly SourceSpecRouteDraft[] {
  return [...routes].sort(compareRoutes);
}

/** The rank this spec declares for one class, or `undefined` when it declares none. */
export function preferenceFor(
  routes: readonly SourceSpecRouteDraft[],
  clientClass: AppClientClass,
): number | undefined {
  return routes.find((route) => route.clientClass === clientClass)?.preference;
}

/**
 * Set one class's rank on a spec, replacing any rank it already declared there.
 *
 * The database's unique index is `(app_id, environment, client_class,
 * preference)` — it forbids the same *pair* twice, not one spec naming a class
 * twice. Listing one spec at both rank 0 and rank 1 therefore reaches the server
 * fine and is merely redundant (the higher rank can never answer, since the spec's
 * state is the same either way). This function is where that redundancy is
 * prevented: setting is a replace, never an append.
 */
export function withRoute(
  routes: readonly SourceSpecRouteDraft[],
  clientClass: AppClientClass,
  preference: number,
): readonly SourceSpecRouteDraft[] {
  const retained = routes.filter((route) => route.clientClass !== clientClass);
  if (preference === ROUTE_UNSERVED) return sortRoutes(retained);
  return sortRoutes([...retained, { clientClass, preference }]);
}

/** Drop a class from a spec entirely (the spec stops serving that client). */
export function withoutRoute(
  routes: readonly SourceSpecRouteDraft[],
  clientClass: AppClientClass,
): readonly SourceSpecRouteDraft[] {
  return withRoute(routes, clientClass, ROUTE_UNSERVED);
}

/**
 * Lowest rank in a class that no spec has claimed.
 *
 * Starts at 1 when rank 0 is taken or when `reserveDefault` is set: rank 0 is the
 * conventional way to *declare* a default, so a caller adding a second candidate
 * wants a fallback rank, not the default slot.
 */
export function nextFreePreference(
  takenPreferences: readonly number[],
  options: { readonly reserveDefault?: boolean } = {},
): number {
  const taken = new Set(takenPreferences);
  let candidate = options.reserveDefault === true ? 1 : 0;
  while (taken.has(candidate)) candidate += 1;
  return candidate;
}

/** Every rank claimed in one class, across all specs. */
export function claimedPreferences(
  specs: readonly SourceSpecRoutingInput[],
  clientClass: AppClientClass,
): readonly number[] {
  return specs
    .flatMap((spec) => spec.clientClassRoutes)
    .filter((route) => route.clientClass === clientClass)
    .map((route) => route.preference);
}

/**
 * A `(class, rank)` pair claimed twice inside one route list.
 *
 * The database rejects this across specs, but a single spec's own list can carry
 * it too — and that never reaches the database because the whole list is written
 * as one row set. Catching it here is what turns a 500 into a field error.
 */
export function findRouteConflict(
  routes: readonly SourceSpecRouteDraft[],
): SourceSpecRouteDraft | undefined {
  const seen = new Set<string>();
  for (const route of sortRoutes(routes)) {
    const key = `${route.clientClass}#${route.preference}`;
    if (seen.has(key)) return route;
    seen.add(key);
  }
  return undefined;
}

/* ------------------------------------------------------------------ *
 * Per-class routing: declared vs effective
 * ------------------------------------------------------------------ */

export interface ClientClassChainEntry {
  readonly specId: string
  readonly specKey: string
  readonly label: string
  readonly preference: number
  readonly specStatus: AppSourceSpecStatus
  readonly sourceStatus: AppSourceBindingStatus
  /**
   * Whether this rank really contributes a routing rule: the spec is `ACTIVE`
   * **and** its source is uploaded. A rank that does not serve is still shown —
   * it is how the operator learns the default slot is dark.
   */
  readonly serves: boolean
}

export interface ClientClassRouting {
  readonly clientClass: AppClientClass
  /** Every rank declared for this class, lowest first. */
  readonly chain: readonly ClientClassChainEntry[]
  /** The lowest rank — what the operator declared as this class's default. */
  readonly declared?: ClientClassChainEntry | undefined
  /** The lowest rank that actually serves. Equals `declared` when that one serves. */
  readonly effective?: ClientClassChainEntry | undefined
  /** `declared` is not the entry that serves ⇒ the request falls through a rank. */
  readonly fallbackApplied: boolean
  /** Chain is non-empty but nobody claimed rank 0. */
  readonly missingDeclaredDefault: boolean
}

/** Does a spec at this state produce a rule? */
export function specServes(spec: Pick<SourceSpecRoutingInput, "status" | "sourceStatus">): boolean {
  return spec.status === "ACTIVE" && spec.sourceStatus === "BOUND";
}

/** Route one class, lowest rank first, resolved against spec state. */
export function describeClientClassRouting(
  specs: readonly SourceSpecRoutingInput[],
  clientClass: AppClientClass,
): ClientClassRouting {
  const chain: ClientClassChainEntry[] = specs
    .flatMap((spec) =>
      spec.clientClassRoutes
        .filter((route) => route.clientClass === clientClass)
        .map((route) => ({
          specId: spec.id,
          specKey: spec.specKey,
          label: spec.label,
          preference: route.preference,
          specStatus: spec.status,
          sourceStatus: spec.sourceStatus,
          serves: specServes(spec),
        })),
    )
    .sort((left, right) =>
      left.preference !== right.preference
        ? left.preference - right.preference
        : left.specKey.localeCompare(right.specKey),
    );

  const declared = chain[0];
  const effective = chain.find((entry) => entry.serves);
  return {
    clientClass,
    chain,
    declared,
    effective,
    fallbackApplied: declared !== undefined && effective !== undefined && declared.specId !== effective.specId,
    missingDeclaredDefault: chain.length > 0 && declared?.preference !== 0,
  };
}

export interface SourceSpecRoutingOverview {
  readonly classes: readonly ClientClassRouting[]
  /** Classes that declare at least one candidate. */
  readonly routedClassCount: number
  /** Classes whose rank-0 slot is claimed. */
  readonly declaredDefaultCount: number
  /** Classes with something that actually serves. */
  readonly effectiveDefaultCount: number
  /** Chain exists, rank 0 unclaimed — the class has candidates but no declared default. */
  readonly missingDeclaredDefault: readonly AppClientClass[]
  /** Rank 0 claimed, but a lower-ranked serving entry answers instead. */
  readonly fallingBack: readonly AppClientClass[]
  /** Nothing serves this class: it depends on the app-level default. */
  readonly dark: readonly AppClientClass[]
  /** No candidate at all: the class is not routed. */
  readonly unrouted: readonly AppClientClass[]
}

/**
 * The whole picture, in display order — what the drawer's first panel renders.
 *
 * Kept as one function rather than four derivations in the component: the four
 * lists are the same walk seen four ways, and recomputing them separately is how
 * they drift (a class listed as `fallingBack` and `dark` at once).
 */
export function describeRoutingOverview(
  specs: readonly SourceSpecRoutingInput[],
): SourceSpecRoutingOverview {
  const classes = APP_CLIENT_CLASSES.map((clientClass) => describeClientClassRouting(specs, clientClass));
  const routed = classes.filter((entry) => entry.chain.length > 0);
  return {
    classes,
    routedClassCount: routed.length,
    declaredDefaultCount: classes.filter((entry) => entry.declared?.preference === 0).length,
    effectiveDefaultCount: classes.filter((entry) => entry.effective !== undefined).length,
    missingDeclaredDefault: classes
      .filter((entry) => entry.missingDeclaredDefault)
      .map((entry) => entry.clientClass),
    fallingBack: classes.filter((entry) => entry.fallbackApplied).map((entry) => entry.clientClass),
    dark: routed.filter((entry) => entry.effective === undefined).map((entry) => entry.clientClass),
    unrouted: classes.filter((entry) => entry.chain.length === 0).map((entry) => entry.clientClass),
  };
}

/* ------------------------------------------------------------------ *
 * Cross-application ledger (the backend-admin operations view)
 * ------------------------------------------------------------------ */

/** One `(spec, client class)` fact, flattened for a ledger. */
export interface SourceSpecRouteFact {
  readonly clientClass: AppClientClass
  readonly preference: number
  /** Rank 0 — the conventional declaration of "this is the class's default". */
  readonly isDeclaredDefault: boolean
  /**
   * This spec is `ACTIVE` **and** has an uploaded source, i.e. the rank really
   * contributes a routing rule. False ranks are kept — the operator needs to see
   * that the declared default is dark.
   */
  readonly serves: boolean
  /**
   * The rank a request for this class actually lands on: the lowest serving rank.
   * At most one route per class carries it.
   */
  readonly isEffective: boolean
}

/** One source spec, flattened — the unit a cross-application ledger lists. */
export interface SourceSpecLedgerEntry {
  readonly specId: string
  readonly specKey: string
  readonly label: string
  readonly runtimeTarget: SdkworkRuntimeTarget
  readonly clientArchitecture: AppClientArchitecture
  readonly handler: "STATIC" | "SPA" | "WIKI"
  readonly status: AppSourceSpecStatus
  readonly sourceStatus: AppSourceBindingStatus
  /** The **app-level** fallback (`deploy_app_source_spec.is_default`), not a per-class default. */
  readonly isAppDefault: boolean
  readonly routes: readonly SourceSpecRouteFact[]
}

/** A spec, plus the fields the ledger shows that routing itself does not read. */
export type SourceSpecLedgerInput = SourceSpecRoutingInput & {
  readonly runtimeTarget: SdkworkRuntimeTarget
  readonly clientArchitecture: AppClientArchitecture
  readonly handler: "STATIC" | "SPA" | "WIKI"
  readonly isDefault: boolean
}

/**
 * Flatten one application's specs into per-`(spec, route)` facts.
 *
 * The backend-admin surface shows the same question the console's drawer does —
 * "which uploaded source serves a phone, and which answers instead" — but across
 * applications instead of within one. It therefore must not re-derive the answer:
 * a second implementation of "declared vs effective" is a second chance to get
 * the `preference === 0` convention and the un-uploaded-spec case wrong, and the
 * two would drift silently because each looks right on its own.
 *
 * So the rule stays here, next to {@link describeClientClassRouting}, and the
 * admin surface renders what this returns. The effective rank is keyed by
 * `specId#preference` rather than by spec: a spec may legally declare the same
 * class at two ranks (redundant, but the server accepts it), and keying by spec
 * alone would then mark both of its rows as "the one that answers".
 */
export function projectSourceSpecLedger(
  specs: readonly SourceSpecLedgerInput[],
): readonly SourceSpecLedgerEntry[] {
  const overview = describeRoutingOverview(specs)
  const effectiveByClass = new Map<AppClientClass, string | undefined>(
    overview.classes.map((entry) => [
      entry.clientClass,
      entry.effective === undefined ? undefined : `${entry.effective.specId}#${entry.effective.preference}`,
    ]),
  )
  return specs.map((spec) => ({
    specId: spec.id,
    specKey: spec.specKey,
    label: spec.label,
    runtimeTarget: spec.runtimeTarget,
    clientArchitecture: spec.clientArchitecture,
    handler: spec.handler,
    status: spec.status,
    sourceStatus: spec.sourceStatus,
    isAppDefault: spec.isDefault,
    routes: sortRoutes(spec.clientClassRoutes).map((route) => ({
      clientClass: route.clientClass,
      preference: route.preference,
      isDeclaredDefault: route.preference === 0,
      serves: specServes(spec),
      isEffective: effectiveByClass.get(route.clientClass) === `${spec.id}#${route.preference}`,
    })),
  }))
}

/* ------------------------------------------------------------------ *
 * Re-pointing a class's default
 * ------------------------------------------------------------------ */

/** One spec's new complete route list, with the version the write must carry. */
export interface SourceSpecRouteUpdate {
  readonly specId: string
  /** `If-Match` value: the spec version the plan was computed from. */
  readonly version: string
  readonly routes: readonly SourceSpecRouteDraft[]
}

export interface ClientClassDefaultPlan {
  readonly updates: readonly SourceSpecRouteUpdate[]
  /** The target already holds rank 0 ⇒ nothing to write. */
  readonly unchanged: boolean
  /** The spec that loses rank 0, demoted to the lowest free rank. */
  readonly demotedSpecId?: string | undefined
}

/**
 * Plan "make `targetSpecId` the default for `clientClass`".
 *
 * Two rules make this more than an assignment:
 *
 * - **Release before claim.** `UNIQUE (app_id, environment, client_class,
 *   preference)` is checked per statement, so writing the new rank 0 before
 *   clearing the old one collides. `updates` is ordered accordingly.
 * - **Demote, do not drop.** The outgoing default slides to the lowest free rank
 *   instead of disappearing, so the class keeps serving while the new default is
 *   still `EMPTY` — which is the whole point of the fallback chain.
 *
 * The plan is computed from the specs as read, so a caller that writes and gets a
 * 409 must re-read and re-plan; the `version` on each update is what makes that
 * failure loud instead of silently clobbering a concurrent edit.
 */
export function planClientClassDefault(
  specs: readonly (SourceSpecRoutingInput & { readonly version: string })[],
  clientClass: AppClientClass,
  targetSpecId: string,
): ClientClassDefaultPlan {
  const target = specs.find((spec) => spec.id === targetSpecId);
  if (target === undefined) return { updates: [], unchanged: true };

  const targetRank = preferenceFor(target.clientClassRoutes, clientClass);
  if (targetRank === 0) return { updates: [], unchanged: true };

  const holder = specs.find(
    (spec) =>
      spec.id !== targetSpecId
      && preferenceFor(spec.clientClassRoutes, clientClass) === 0,
  );

  // Ranks the holder may NOT move into, i.e. everything occupied at the moment
  // statement 1 runs. Only the holder's own entry is free to reuse; **everyone
  // else's stays put, including the target's**.
  //
  // Excluding the target here is the trap: it looks justified ("the target is on
  // its way to rank 0") but statement 1 executes while the target is still on its
  // old rank, and the database checks `UNIQUE (app_id, environment,
  // client_class, preference)` per statement. The canonical two-spec case —
  // pc at 0, pc-alt at 1, "make pc-alt the default" — then plans pc → 1, which
  // collides with the pc-alt row that has not moved yet, so the most common
  // re-point fails outright. The target only vacates in statement 2.
  //
  // The price is that the holder may skip past a rank the target is about to
  // free (0,1 → pc-alt at 0, pc at 2). Ranks are explicitly allowed to be sparse
  // (§8), and re-pointing back re-compacts the chain, so a gap is the right trade
  // for a plan that cannot be rejected.
  const takenWhenHolderMoves = [
    0,
    ...specs
      .filter((spec) => spec.id !== holder?.id)
      .flatMap((spec) => spec.clientClassRoutes)
      .filter((route) => route.clientClass === clientClass)
      .map((route) => route.preference),
  ];
  const demotedRank = nextFreePreference(takenWhenHolderMoves, { reserveDefault: true });

  const updates: SourceSpecRouteUpdate[] = [];
  if (holder !== undefined) {
    updates.push({
      specId: holder.id,
      version: holder.version,
      routes: withRoute(holder.clientClassRoutes, clientClass, demotedRank),
    });
  }
  updates.push({
    specId: target.id,
    version: target.version,
    routes: withRoute(target.clientClassRoutes, clientClass, 0),
  });

  return {
    updates,
    unchanged: false,
    demotedSpecId: holder?.id,
  };
}

/* ------------------------------------------------------------------ *
 * Route list ↔ form options
 * ------------------------------------------------------------------ */

/**
 * The four ranks a single spec may pick for one class.
 *
 * A closed, small list on purpose: the panel's job is "who is the default and who
 * is next", and a free numeric field invites ranks like 9000 that read as
 * meaningful and are not.
 */
export const ROUTE_RANK_OPTIONS = [0, 1, 2, 3] as const

/**
 * The rank a spec should show for a class, given what its siblings already claim.
 *
 * `undefined` means "not served". A rank the spec already holds is always kept,
 * even when it sits outside {@link ROUTE_RANK_OPTIONS} — a spec set to rank 7 by
 * an older client must still render as rank 7 rather than silently snapping to
 * "not served" (which would delete the route on the next save).
 */
export function offeredRanks(
  spec: Pick<SourceSpecRoutingInput, "clientClassRoutes"> & { readonly id?: string | undefined },
  clientClass: AppClientClass,
  siblings: readonly SourceSpecRoutingInput[],
): readonly number[] {
  const own = preferenceFor(spec.clientClassRoutes, clientClass);
  const takenByOthers = new Set(
    siblings
      .filter((other) => other.id !== spec.id)
      .flatMap((other) => other.clientClassRoutes)
      .filter((route) => route.clientClass === clientClass)
      .map((route) => route.preference),
  );
  // 显式标回 `readonly number[]`：`ROUTE_RANK_OPTIONS` 是 `as const` 元组，直接
  // filter 出来的是字面量联合数组，之后 `ranks.includes(own)`（own 为 number）会
  // 被判成"参数不在联合里"。这里要的就是「一组 rank」，不是「0..3 的闭集」。
  const ranks: readonly number[] = ROUTE_RANK_OPTIONS.filter((rank) => rank === own || !takenByOthers.has(rank));
  return own !== undefined && !ranks.includes(own)
    ? [...ranks, own].sort((left, right) => left - right)
    : ranks;
}

/* ------------------------------------------------------------------ *
 * Draft validation
 * ------------------------------------------------------------------ */

/** The form's model of one spec, create or edit. */
export interface SourceSpecDraft {
  readonly specKey: string
  readonly label: string
  readonly runtimeTarget: SdkworkRuntimeTarget
  readonly clientArchitecture: AppClientArchitecture
  readonly handler: "STATIC" | "SPA" | "WIKI"
  readonly pathPrefix: string
  readonly indexFiles: string
  readonly spaFallback: string
  /** `deploy_app_source_spec.is_default` — the app-level fallback, not a per-class default. */
  readonly isDefault: boolean
  readonly status: AppSourceSpecStatus
  readonly routes: readonly SourceSpecRouteDraft[]
}

/**
 * A new spec's starting point.
 *
 * `browser` + `react` rather than an empty runtime target: those are the values
 * the column defaults to, and mirroring them means the form opens showing what
 * the row would actually be rather than a placeholder that differs from the
 * server's own default.
 */
export const EMPTY_SOURCE_SPEC_DRAFT: SourceSpecDraft = {
  specKey: "",
  label: "",
  runtimeTarget: "browser",
  clientArchitecture: "react",
  handler: "SPA",
  pathPrefix: "/",
  indexFiles: "index.html",
  spaFallback: "/index.html",
  isDefault: false,
  status: "ACTIVE",
  routes: [],
}

export type SourceSpecDraftIssue =
  | "specKeyRequired"
  | "specKeyInvalid"
  | "specKeyTaken"
  | "labelRequired"
  | "routeClassNameUnknown"
  | "routeRankNegative"
  | "routeDuplicate"

/**
 * Everything the console can decide on its own about a draft.
 *
 * Deliberately narrow: it checks what the contract states (`CompositionKey`'s
 * shape, the one-route-per-class invariant) and nothing about `pathPrefix` /
 * `indexFiles` semantics, which the delivery plane owns. Guessing at those here
 * would let the form refuse a value the server accepts.
 */
export function validateSourceSpecDraft(
  draft: SourceSpecDraft,
  siblings: readonly Pick<SourceSpecRoutingInput, "id" | "specKey">[],
): readonly SourceSpecDraftIssue[] {
  const issues: SourceSpecDraftIssue[] = [];
  const key = draft.specKey.trim();
  if (key === "") issues.push("specKeyRequired");
  else if (!SOURCE_SPEC_KEY_PATTERN.test(key)) issues.push("specKeyInvalid");
  else if (siblings.some((sibling) => sibling.specKey === key)) issues.push("specKeyTaken");

  if (draft.label.trim() === "") issues.push("labelRequired");

  const seen = new Set<string>();
  let duplicate = false;
  for (const route of draft.routes) {
    if (!APP_CLIENT_CLASSES.includes(route.clientClass)) issues.push("routeClassNameUnknown");
    if (!Number.isInteger(route.preference) || route.preference < 0) issues.push("routeRankNegative");
    const pair = `${route.clientClass}#${route.preference}`;
    if (seen.has(pair)) duplicate = true;
    seen.add(pair);
  }
  if (duplicate) issues.push("routeDuplicate");

  // De-duplicated so a caller can render one message per distinct problem even
  // when two routes trip the same check.
  return [...new Set(issues)];
}

/**
 * `indexFiles` is one text field; the wire wants a list of non-empty names.
 *
 * Returns `readonly` — this array is a parsed view of a form field, not a place
 * to mutate. The generated request type declares `string[]` (it is what the
 * server deserialises into), so the two call sites spread the result into a
 * fresh mutable array at the boundary rather than weakening this contract.
 */
export function splitIndexFiles(value: string): readonly string[] {
  return value
    .split(/[,\s]+/)
    .map((entry) => entry.trim())
    .filter((entry) => entry !== "");
}

/** Read an existing spec back into the form. */
export function sourceSpecDraftFrom(spec: SourceSpecRoutingInput & {
  readonly runtimeTarget: SdkworkRuntimeTarget
  readonly clientArchitecture: AppClientArchitecture
  readonly handler: "STATIC" | "SPA" | "WIKI"
  readonly pathPrefix: string
  readonly indexFiles: readonly string[]
  readonly spaFallback?: string | undefined
  readonly isDefault: boolean
}): SourceSpecDraft {
  return {
    specKey: spec.specKey,
    label: spec.label,
    runtimeTarget: spec.runtimeTarget,
    clientArchitecture: spec.clientArchitecture,
    handler: spec.handler,
    pathPrefix: spec.pathPrefix,
    indexFiles: spec.indexFiles.join(", "),
    spaFallback: spec.spaFallback ?? "",
    isDefault: spec.isDefault,
    status: spec.status,
    routes: sortRoutes(spec.clientClassRoutes).map((route) => ({
      clientClass: route.clientClass,
      preference: route.preference,
    })),
  }
}
