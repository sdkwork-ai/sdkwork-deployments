/**
 * Transport built from the console's injected SDK client.
 *
 * The surfaces in this package never construct a client: they receive a
 * transport (see `./application-usage` and `./deploy-application-detail`). This
 * module is the one place that turns an already-created
 * `SdkworkDeployAppClient` into both transports, so a consumer writes one line
 * (`createDeployApplicationDetailTransport(client)`) instead of re-implementing
 * the mapping, and a test builds the same shape from a fake client.
 *
 * The client type comes from the console core's `/sdk` boundary, not from the
 * generated SDK package (`FRONTEND_SPEC.md` §1.2: capability packages consume SDK
 * access through core public exports).
 */
import type { SdkworkDeployAppClient } from "@sdkwork/deployments-pc-console-core/sdk";

import type { ApplicationUsageTransport } from "./application-usage";
import type { DeployApplicationDetailTransport } from "./deploy-application-detail";

/**
 * Both planes over one client.
 *
 * They share a client because they address the same deploy app surface; they
 * stay separate types because they mean different things (delivery state versus
 * metered consumption), and a consumer may supply only one of them.
 */
export type DeployApplicationDetailClientTransport =
  & DeployApplicationDetailTransport
  & ApplicationUsageTransport;

/**
 * Map the deploy client onto the read ports.
 *
 * Every method is a direct pass-through: normalization belongs to
 * `loadDeployApplicationDetail` / `loadApplicationUsage`, so this mapping stays
 * trivial and immune to contract drift.
 */
export function createDeployApplicationDetailTransport(
  client: SdkworkDeployAppClient,
): DeployApplicationDetailClientTransport {
  return {
    listDeployments: (appId, input) => client.deployment.list(appId, input),
    listDomains: (appId) => client.app.domains.list(appId),
    listPackages: (appId, input) => client.package.list(appId, input),
    listReleases: (appId, input) => client.release.list(appId, input),
    listUsageEvents: (input) => client.usage.usageEvents.list(input),
    retrieveApp: (appId) => client.app.retrieve(appId),
    retrieveReadiness: (appId) => client.app.readiness.retrieve(appId),
  };
}
