import type { DeploymentsPcModuleDefinition } from "@sdkwork/deployments-pc-commons";

/**
 * Admin control plane for the app template marketplace. The pages themselves
 * are the shared generic `Page` driven by the admin registry (see
 * `createDeploymentsAdminRegistry`): categories are plain CRUD, listings are
 * moderation transitions over the selection, purchases revoke an entitlement
 * (the grant itself comes from acquire or commerce fulfillment).
 */
export const deploymentsModule = {
  id: "app-templates",
  label: "app-templates",
  surface: "backend-admin",
  entries: [
    { resource: "templateCategories", label: "Template categories", description: "Marketplace taxonomy for app templates", permission: "deploy.templateCategories.read", order: 6 },
    { resource: "appTemplates", label: "App templates", description: "Author-published listings and moderation", permission: "deploy.appTemplates.read", order: 7 },
    { resource: "templatePurchases", label: "Template purchases", description: "Acquisition entitlements and revocations", permission: "deploy.templatePurchases.read", order: 8 },
  ],
} as const satisfies DeploymentsPcModuleDefinition;
