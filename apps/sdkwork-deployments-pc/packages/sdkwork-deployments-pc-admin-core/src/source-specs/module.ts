import type { DeploymentsPcModuleDefinition } from "@sdkwork/deployments-pc-commons";

/**
 * The backend-admin module for the source-specs ledger.
 *
 * `permission` is `deploy.apps.read` and not a new one: this view reads the
 * application ledger and its specs, which is exactly what that permission gates
 * on the console side. Inventing a `deploy.source_specs.read` here would create a
 * second key for one capability, and the two would then have to be granted
 * together forever.
 *
 * `order: 16` slots it after the operations group: the application center owns
 * 1-4, infrastructure 11-14 (nginx, clusters, nodes, audit in their own modules),
 * `localProjects` 15, and the download catalog 19, so the menu keeps one stable
 * order when it is assembled from several modules.
 */
export const sourceSpecsModule = {
  id: "source-specs",
  label: "source specs",
  surface: "backend-admin",
  entries: [
    {
      resource: "sourceSpecs",
      label: "Source specs",
      description: "Which uploaded source serves each kind of client, per application",
      permission: "deploy.apps.read",
      order: 16,
    },
  ],
} as const satisfies DeploymentsPcModuleDefinition;
