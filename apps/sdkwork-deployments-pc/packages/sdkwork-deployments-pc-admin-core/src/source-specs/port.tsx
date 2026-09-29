/**
 * The read face of the backend-admin 「源码规格」 view.
 *
 * ## Why this is a port and not an SDK client
 *
 * Source specs live on the **app plane** (`/app/v3/api`), and `admin-core`'s own
 * contract says backend-admin code reaches the server only through the
 * backend-admin SDK — which has no `app` surface at all (`nginx` / `server` /
 * `cluster` / `audit` / `entitlement` / `buildQueue` / `runners` / `tls` /
 * `retention` / `usage` / `signingHealth` / `sourceEvents`, and `sourceEvents` is
 * Git webhooks, not specs). There is therefore no backend-admin endpoint that can
 * answer this question today.
 *
 * Rather than reach around the boundary — or add an SDK dependency to a package
 * the workspace materialiser generates — the view takes a **structural port**,
 * exactly as `admin-local-projects` takes a `SandboxExplorerPort`. The app shell,
 * which is the composition root and already holds both clients, builds the
 * adapter (`src/ports/source-specs-admin-port.ts`). Consequences worth stating:
 *
 * - `admin-core` gains no dependency and this file imports nothing but `react`.
 * - The rule that decides *who answers a client* stays in the publishing
 *   capability's routing kernel, which is where it is tested
 *   (`projectSourceSpecLedger`); the adapter projects it, the page only renders.
 * - Swapping the adapter for a future backend-admin endpoint is a shell-level
 *   change with no edit here.
 *
 * ## Why labels are in the payload
 *
 * The row carries both the raw enum value and its localised label. The raw value
 * is what a filter compares against; the label comes from the owning capability's
 * `Record<Union, key>` tables. Re-declaring those vocabularies here is the failure
 * mode this avoids: a sixteenth `runtimeTarget` would then render as a raw token
 * on one surface and as copy on the other.
 */
import { createContext, useContext, type ReactNode } from "react";

/** One `(source spec, client class)` fact as the ledger shows it. */
export interface SourceSpecsAdminRoute {
  readonly clientClass: string
  readonly clientClassLabel: string
  readonly preference: number
  /** Rank 0 — the conventional declaration of "this is the class's default". */
  readonly isDeclaredDefault: boolean
  /** This spec is ACTIVE with a source attached, i.e. it produces a routing rule. */
  readonly serves: boolean
  /** The rank a request for this class actually lands on. At most one per class. */
  readonly isEffective: boolean
}

/** One source spec. */
export interface SourceSpecsAdminSpec {
  readonly specId: string
  readonly specKey: string
  readonly label: string
  readonly runtimeTargetLabel: string
  readonly clientArchitectureLabel: string
  readonly handlerLabel: string
  readonly statusLabel: string
  readonly sourceStatusLabel: string
  /** Not `ACTIVE` — it will not serve even after a source is attached. */
  readonly disabled: boolean
  /**
   * No source attached ⇒ **no routing rule at all**, whatever the ranks say. This
   * is the fact that makes "the declared default" and "the source that answers"
   * different things, so it travels as its own field rather than being inferred
   * from the label.
   */
  readonly silent: boolean
  /** The app-level fallback, not a per-class default. */
  readonly isAppDefault: boolean
  readonly routes: readonly SourceSpecsAdminRoute[]
}

/** One application and its specs in the selected environment. */
export interface SourceSpecsAdminApplication {
  readonly appId: string
  readonly appName: string
  readonly appStatusLabel: string
  readonly specs: readonly SourceSpecsAdminSpec[]
}

export interface SourceSpecsAdminPage {
  readonly applications: readonly SourceSpecsAdminApplication[]
  readonly page: number
  readonly hasMore: boolean
  /**
   * Names of applications whose spec list could not be read.
   *
   * A partial failure must be *visible*: an application silently missing from a
   * cross-application ledger reads as "this app has no specs", which is a
   * different and much more actionable statement.
   */
  readonly unreadableApplications: readonly string[]
}

export interface SourceSpecsAdminPort {
  /**
   * Environments the picker offers, in the order the owning capability declares
   * them.
   *
   * The list travels with the port rather than being written here because it is
   * the contract's own closed union (`AppPublishEnvironment`), and the node
   * inside a `select` is the wrong place for a vocabulary that the server
   * rejects. The console learned this the hard way: its picker used to offer a
   * value the create endpoint refuses, because the shared list was never
   * compared against the contract — see `APP_PUBLISH_ENVIRONMENTS`.
   */
  readonly environments: readonly string[]
  /**
   * What the picker opens on when no application has been chosen yet.
   *
   * A ledger has no "current app" to read `defaultEnvironment` from, so the host
   * names one. It is a *starting point*, not a fallback: the page renders it as
   * the selected value and never substitutes it behind the operator's back,
   * which would answer a different question than the one asked.
   */
  readonly defaultEnvironment: string
  /**
   * One page of applications, each with its specs.
   *
   * `environment` is required: specs are scoped by it, and a ledger that mixed
   * environments would show two "Desktop defaults" for one app.
   */
  list(input: { readonly page: number; readonly environment: string }): Promise<SourceSpecsAdminPage>
}

const Context = createContext<SourceSpecsAdminPort | null>(null);

export function SourceSpecsAdminPortProvider({
  children,
  port,
}: {
  children: ReactNode
  port: SourceSpecsAdminPort | null
}) {
  return <Context.Provider value={port}>{children}</Context.Provider>;
}

/**
 * The port, or `null` when the host did not supply one.
 *
 * `null` rather than a throw: unlike an SDK client, a missing port is a
 * configuration fact the page can explain on screen ("not available in this
 * session"), and `admin-local-projects` already renders its equivalent that way.
 */
export function useSourceSpecsAdminPort(): SourceSpecsAdminPort | null {
  return useContext(Context);
}
