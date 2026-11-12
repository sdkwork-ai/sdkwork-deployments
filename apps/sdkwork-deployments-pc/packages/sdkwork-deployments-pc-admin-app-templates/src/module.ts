import type { DeploymentsPcModuleDefinition } from "@sdkwork/deployments-pc-commons";
export const deploymentsModule = { id: "app-center", label: "app center", surface: "backend-admin",
  // Admin control plane for the application center. The applications ledger is
  // the canonical `PublishingAppsPage` mounted with `surface="admin"`; the
  // catalog resources are the shared generic table driven by the admin registry
  // (see `createDeploymentsAdminRegistry`): categories are plain CRUD, listings
  // pair app-plane authoring with backend-admin moderation, versions are a
  // scoped read of one listing's history. Trade is not part of this surface —
  // app-template orders belong to the platform order center — so the module
  // contributes catalog resources only.
entries: [
    { resource: "applications", label: "Applications", description: "Manage deploy_app applications", permission: "deploy.apps.read", order: 1 },
    { resource: "appTemplates", label: "App templates", description: "Author-published listings and moderation", permission: "deploy.appTemplates.read", order: 2 },
    { resource: "templateCategories", label: "Template categories", description: "Marketplace taxonomy for app templates", permission: "deploy.templateCategories.read", order: 3 },
    { resource: "appTemplateVersions", label: "Template versions", description: "Published version snapshots of one listing", permission: "deploy.appTemplateVersions.read", order: 4 }
] } as const satisfies DeploymentsPcModuleDefinition;
