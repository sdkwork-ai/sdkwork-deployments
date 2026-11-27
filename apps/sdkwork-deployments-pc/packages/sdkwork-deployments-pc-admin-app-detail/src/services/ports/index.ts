/**
 * Injection ports of the deployment-application detail package.
 *
 * The package owns the read model, the page, and the copy of one deployment
 * application. Two things stay outside it on purpose:
 *
 * - **where the reads come from.** An SDK client is bound by its consumer's
 *   runtime, so the package defines the call shape
 *   ({@link DeployApplicationDetailTransport}) and the deployments console
 *   supplies the concrete instance. That keeps the package free of base URLs,
 *   credentials, and locale wiring (`FRONTEND_SPEC.md` §3).
 * - **where the commands go.** Configuration, source/remix, access-token
 *   issuance, and endpoint-kind correction are owned by whichever console opened
 *   the page, so the page calls into {@link DeploymentApplicationDetailCapabilityPort}
 *   instead of routing or re-implementing a write.
 *
 * A member that is `undefined` means "not offered by this host": the page simply
 * does not render that control or that meter.
 */
import type { DeployApplicationDetailRecord } from "../../types/application-detail";
import type {
  ApplicationUsageSnapshot,
  ApplicationUsageSourceFailure,
  ApplicationUsageWindow,
} from "../../types/application-usage";

/** Selects which application and which metering window to read. */
export interface DeploymentApplicationDetailReadQuery {
  /** Deploy application uuid (`AppResponse.id`) — never the decimal id. */
  appId: string;
  /** Canonical hostnames the caller already knows, for traffic attribution. */
  hostnames?: readonly string[];
  /** Rolling window the usage plane covers. */
  window: ApplicationUsageWindow;
  /** Injectable clock for deterministic windows. */
  now?: Date;
}

/** The usage read, including the sources that failed so the page can explain. */
export interface DeploymentApplicationUsageRead {
  failures: readonly ApplicationUsageSourceFailure[];
  snapshot: ApplicationUsageSnapshot;
}

/**
 * Reads of the detail surface.
 *
 * `loadDetail` is mandatory: without a fact sheet there is no page. `loadUsage`
 * is optional, and a host that omits it renders the consumption plane as "not
 * connected" rather than as zero.
 */
export interface DeploymentApplicationDetailDataSource {
  /** The application fact sheet and its delivery state. */
  loadDetail: (query: { appId: string }) => Promise<DeployApplicationDetailRecord>;
  /** Traffic, service/API, and storage consumption for the selected window. */
  loadUsage?: (query: DeploymentApplicationDetailReadQuery) => Promise<DeploymentApplicationUsageRead>;
}

/**
 * What the operator may open.
 *
 * The page never decides authorization — the backend does — and this port only
 * keeps a control out of the header when the host already knows the grant is
 * absent. An `undefined` member means "not declared, show it".
 */
export interface DeploymentApplicationDetailPermissionPort {
  /** `deploy.apps.read`: the source/remix surface of the application. */
  sourceRead?: () => boolean;
  /** `deploy.apps.write`: source/remix writes opened from the detail page. */
  sourceWrite?: () => boolean;
}

/**
 * Commands the host owns.
 *
 * Each callback opens the operation in the console that owns it. Re-implementing
 * one here would fork the write path, so the page only offers the ones supplied.
 */
export interface DeploymentApplicationDetailCapabilityPort {
  /** Application configuration (official account / mini program / payment). */
  openConfiguration?: (appId: string) => void;
  /** Delegated access-token issuance. */
  openAccessToken?: (appId: string) => void;
  /** Draft endpoint-kind correction. */
  openKindCorrection?: (appId: string) => void;
  /** Source and remix. */
  openSource?: (appId: string) => void;
}
