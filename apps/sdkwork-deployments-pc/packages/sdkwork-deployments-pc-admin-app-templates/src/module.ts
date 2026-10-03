import type { DeploymentsPcModuleDefinition } from "@sdkwork/deployments-pc-commons";
export const deploymentsModule = { id: "app-templates", label: "app templates", surface: "backend-admin",
  // Admin control plane for the app template catalog. The pages themselves are
  // the shared generic table driven by the admin registry (see
  // `createDeploymentsAdminRegistry`): categories are plain CRUD, listings are
  // moderation transitions over the selection, versions are a scoped read of one
  // listing's history. Trade is not part of this surface — app-template orders
  // belong to the platform order center — so the module contributes catalog
  // resources only.
entries: [
    { resource: "templateCategories", label: "Template categories", description: "Marketplace taxonomy for app templates", permission: "deploy.templateCategories.read", order: 6 },
    { resource: "appTemplates", label: "App templates", description: "Author-published listings and moderation", permission: "deploy.appTemplates.read", order: 7 },
    { resource: "appTemplateVersions", label: "Template versions", description: "Published version snapshots of one listing", permission: "deploy.appTemplateVersions.read", order: 8 }
] } as const satisfies DeploymentsPcModuleDefinition;
