import type { DeploymentsPcModuleDefinition } from "@sdkwork/deployments-pc-commons";

export const deploymentsModule = {
  id: "local-projects",
  label: "local-projects",
  surface: "backend-admin",
  entries: [
    {
      resource: "localProjects",
      label: "Local Projects",
      description: "Browse Docker deploy modules and local runtime nodes under the Drive deploy sandbox",
      permission: "deploy.local_projects.read",
      order: 15,
    },
  ],
} as const satisfies DeploymentsPcModuleDefinition;
