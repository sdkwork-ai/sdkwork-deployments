import type { DeploymentsPcModuleDefinition } from "@sdkwork/deployments-pc-commons";

/**
 * App-center backend-admin module.
 *
 * The pages are the shared generic table driven by the admin registry
 * (`createDeploymentsAdminRegistry`): categories are plain CRUD, application
 * scaffolds are moderation transitions over the selection, and scaffold
 * versions are a scoped read of one scaffold's history. Trade is not part of
 * this surface — application-scaffold orders belong to the platform order
 * center — so the module contributes catalog resources only.
 */
export const deploymentsModule = {
  id: "app-center",
  label: "app center",
  surface: "backend-admin",
  entries: [
    { resource: "applications", label: "Applications", description: "Deployment application ledger", permission: "deploy.apps.read", order: 1 },
    { resource: "scaffolds", label: "Application scaffolds", description: "Author-published application scaffolds and moderation", permission: "deploy.scaffolds.read", order: 2 },
    { resource: "scaffoldCategories", label: "Scaffold categories", description: "Application scaffold marketplace taxonomy", permission: "deploy.scaffoldCategories.read", order: 3 },
    { resource: "scaffoldVersions", label: "Scaffold versions", description: "Published version snapshots of one scaffold", permission: "deploy.scaffolds.read", order: 4 },
  ],
} as const satisfies DeploymentsPcModuleDefinition;
