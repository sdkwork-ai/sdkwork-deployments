/**
 * Source-spec templates — the console's knowledge of "what a source spec for a
 * PC web app / an H5 app / a mini program actually is".
 *
 * ## Why a new spec should not be a blank form
 *
 * `deploy_app_source_spec` carries nine fields that decide routing and serving:
 * `specKey`, `runtimeTarget`, `clientArchitecture`, `handler`, `indexFiles`,
 * `pathPrefix`, the client-class routes, `priority` and `isDefault`. Every one of
 * them is a *vocabulary* value the operator cannot be expected to know — nobody
 * arrives knowing that an H5 bundle and a PC bundle share `runtimeTarget =
 * browser` and differ only in `clientArchitecture`, or that `specKey` is a
 * projected variant key that must be lower-case.
 *
 * Left as a blank form, that knowledge has to be recalled correctly on every
 * declaration, and the failure mode is silent: a spec declared with the wrong
 * `runtimeTarget` still saves, still accepts a source, and simply never matches
 * the client it was meant for. So the declaration starts from a **template** —
 * the operator names the *client* and the console fills in the vocabulary. The
 * full field set stays reachable in the source-specs drawer for the minority of
 * cases that genuinely need it.
 *
 * ## The two rules this module exists to enforce
 *
 * 1. **`specKey` must be one the database accepts.** The contract's
 *    `CompositionKey` allows `[A-Za-z0-9._:-]`, but the column CHECK on
 *    `deploy_app_source_spec.spec_key` is stricter: lower-case only, no colon, and
 *    it must start with `[a-z0-9]`. A key that satisfies the contract but not the
 *    column is rejected at the database with nothing on screen explaining why.
 *    Every template therefore ships a key that satisfies *both*, and
 *    {@link SOURCE_SPEC_KEY_SAFE_PATTERN} is the intersect-on check for anything
 *    derived from user input.
 * 2. **A template must not collide with a spec the app already has.** The wire
 *    enforces two uniqueness rules — `(app, environment, specKey)` on the spec and
 *    `(app, environment, clientClass, preference)` on the route — so a template
 *    applied twice would be rejected rather than merely redundant. The planner
 *    below resolves both against the app's current specs, which is what lets the
 *    dialog offer "declare and continue" without the operator having to invent a
 *    free key or rank.
 *
 * Pure and structural: the planner reads only the slice of `AppSourceSpecResponse`
 * that decides a collision, so it is testable without a renderer or a live API.
 */
import type {
  AppClientArchitecture,
  AppClientClass,
  AppPublishEnvironment,
  CreateAppSourceSpecRequest,
  SdkworkRuntimeTarget,
} from "@sdkwork/deployments-pc-console-core/sdk";
import type { PublishingMessageKey } from "../i18n.ts";
import {
  claimedPreferences,
  nextFreePreference,
  type SourceSpecRoutingInput,
} from "./app-source-spec-routing.ts";

/**
 * The narrowest `specKey` rule any layer enforces — the column CHECK, intersected
 * with the contract's `CompositionKey`.
 *
 * The contract is the wider of the two, so checking against the contract alone
 * lets through `PC` and `my:spec`, both of which the column rejects. Stated here
 * rather than reused from `app-source-spec-routing.ts`'s `SOURCE_SPEC_KEY_PATTERN`
 * on purpose: that constant mirrors the *contract*, and the drawer that shows it
 * has a server round trip to explain a rejection. This one gates values the
 * console *generates*, where a rejection would be a bug in the console rather
 * than a correction for the operator.
 */
export const SOURCE_SPEC_KEY_SAFE_PATTERN = /^[a-z0-9][a-z0-9._-]{0,63}$/;

/** One "declare this kind of source" preset. */
export interface SourceSpecTemplate {
  /** Stable identity of the template itself (not the spec it creates). */
  readonly id: string
  /** Operator-facing name of the client this preset serves. */
  readonly labelKey: PublishingMessageKey
  /** One line on what the preset assumes, shown under the name. */
  readonly hintKey: PublishingMessageKey
  /** Contract- and column-legal key; de-duplicated by the planner. */
  readonly specKey: string
  readonly runtimeTarget: SdkworkRuntimeTarget
  readonly clientArchitecture: AppClientArchitecture
  readonly handler: "STATIC" | "SPA" | "WIKI"
  readonly indexFiles: readonly string[]
  /** The client class this preset claims by default. */
  readonly clientClass: AppClientClass
}

/**
 * The presets, in the order the console shows them: the web pair first (they are
 * the common case and the pair whose distinction is least guessable), then the
 * device roots.
 *
 * Every `specKey` here is already within {@link SOURCE_SPEC_KEY_SAFE_PATTERN}; the
 * planner de-duplicates but never has to repair one.
 */
export const SOURCE_SPEC_TEMPLATES: readonly SourceSpecTemplate[] = [
  {
    id: "pc-web",
    labelKey: "specTemplatePcWeb",
    hintKey: "specTemplatePcWebHint",
    specKey: "pc",
    runtimeTarget: "browser",
    clientArchitecture: "react",
    handler: "SPA",
    indexFiles: ["index.html"],
    clientClass: "DESKTOP",
  },
  {
    id: "h5-web",
    labelKey: "specTemplateH5Web",
    hintKey: "specTemplateH5WebHint",
    specKey: "h5",
    runtimeTarget: "browser",
    clientArchitecture: "react-h5",
    handler: "SPA",
    indexFiles: ["index.html"],
    clientClass: "MOBILE",
  },
  {
    id: "static-web",
    labelKey: "specTemplateStaticWeb",
    hintKey: "specTemplateStaticWebHint",
    specKey: "web",
    runtimeTarget: "browser",
    clientArchitecture: "static-web",
    handler: "STATIC",
    indexFiles: ["index.html"],
    clientClass: "DESKTOP",
  },
  {
    id: "mini-program",
    labelKey: "specTemplateMiniProgram",
    hintKey: "specTemplateMiniProgramHint",
    specKey: "mini-program",
    runtimeTarget: "mini-program",
    clientArchitecture: "mini-program",
    handler: "STATIC",
    indexFiles: [],
    clientClass: "MOBILE",
  },
  {
    id: "flutter-mobile",
    labelKey: "specTemplateFlutter",
    hintKey: "specTemplateFlutterHint",
    specKey: "flutter",
    runtimeTarget: "flutter-android",
    clientArchitecture: "flutter-mobile",
    handler: "STATIC",
    indexFiles: [],
    clientClass: "MOBILE",
  },
  {
    id: "android-native",
    labelKey: "specTemplateAndroid",
    hintKey: "specTemplateAndroidHint",
    specKey: "android",
    runtimeTarget: "android-native",
    clientArchitecture: "android-mobile",
    handler: "STATIC",
    indexFiles: [],
    clientClass: "MOBILE",
  },
  {
    id: "ios-native",
    labelKey: "specTemplateIos",
    hintKey: "specTemplateIosHint",
    specKey: "ios",
    runtimeTarget: "ios-native",
    clientArchitecture: "ios-mobile",
    handler: "STATIC",
    indexFiles: [],
    clientClass: "MOBILE",
  },
  {
    id: "harmony-native",
    labelKey: "specTemplateHarmony",
    hintKey: "specTemplateHarmonyHint",
    specKey: "harmony",
    runtimeTarget: "harmony-native",
    clientArchitecture: "harmony-mobile",
    handler: "STATIC",
    indexFiles: [],
    clientClass: "MOBILE",
  },
]

/** Look one preset up by id (the value a rendered card carries). */
export function sourceSpecTemplateById(id: string): SourceSpecTemplate | undefined {
  return SOURCE_SPEC_TEMPLATES.find((template) => template.id === id)
}

/**
 * The slice of `AppSourceSpecResponse` the planner reads.
 *
 * Deliberately **wider** than `SourceSpecRoutingInput`: the routing view carries
 * only what decides routing, while `isDefault` is a spec-level fact about which
 * variant a client matching no class falls back to. Reusing the routing slice
 * here would have meant either widening that type for one caller or dropping the
 * app-level default from the plan — the first spreads one module's concern, the
 * second would make a second declaration violate
 * `uk_deploy_app_source_spec_default`.
 *
 * Structural so a test needs no timestamps or version.
 */
export type SourceSpecTemplateScope = SourceSpecRoutingInput & {
  readonly isDefault: boolean
}

/**
 * First key of the form `base`, `base-2`, `base-3`, … that no spec has taken.
 *
 * `uk_deploy_app_source_spec_key` is `(app_id, environment, spec_key)`, so the
 * second PC preset in one environment has to be spelled differently rather than
 * rejected. Suffixing keeps the key recognisable — the alternative (a random
 * suffix) trades a conflict for an unreadable ledger.
 *
 * The `-{n}` spelling stays inside the safe pattern for every `n`.
 */
export function uniqueSpecKey(base: string, taken: readonly string[]): string {
  const used = new Set(taken)
  if (!used.has(base)) return base
  let suffix = 2
  while (used.has(`${base}-${suffix}`)) suffix += 1
  return `${base}-${suffix}`
}

/**
 * Turn a preset into the create request the contract expects.
 *
 * `label` is passed in rather than translated here: this module is pure and the
 * caller already holds the translator, which keeps the whole file free of
 * locale state.
 *
 * Two facts are resolved against the app's **current** specs, because both are
 * wire constraints rather than preferences:
 *
 * - the key, against `(app, environment, spec_key)`;
 * - the rank, against `(app, environment, client_class, preference)`. A template
 *   arriving second for a class takes the next free rank — so it becomes that
 *   class's *fallback*, which is what an operator adding a second candidate
 *   means. Passing `reserveDefault` would be wrong here: the intent of "add
 *   another spec" is a candidate behind the existing one, not a new default.
 *
 * `isDefault` is the **app-level** flag (the variant a client matching no class
 * falls back to), which is a different fact from a class default. It is set only
 * when the app has no active default yet, so a second declaration cannot violate
 * `uk_deploy_app_source_spec_default`.
 */
export function planSourceSpecFromTemplate(
  template: SourceSpecTemplate,
  environment: AppPublishEnvironment,
  specs: readonly SourceSpecTemplateScope[],
  label: string,
): CreateAppSourceSpecRequest {
  const hasActiveDefault = specs.some((spec) => spec.isDefault && spec.status === "ACTIVE")
  return {
    environment,
    specKey: uniqueSpecKey(template.specKey, specs.map((spec) => spec.specKey)),
    label,
    runtimeTarget: template.runtimeTarget,
    clientArchitecture: template.clientArchitecture,
    clientClassRoutes: [{
      clientClass: template.clientClass,
      preference: nextFreePreference(claimedPreferences(specs, template.clientClass)),
    }],
    pathPrefix: "/",
    handler: template.handler,
    indexFiles: [...template.indexFiles],
    isDefault: !hasActiveDefault,
    priority: 0,
  }
}
