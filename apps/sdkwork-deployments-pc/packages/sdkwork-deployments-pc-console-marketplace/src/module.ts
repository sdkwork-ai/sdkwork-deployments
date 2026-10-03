import type { DeploymentsPcModuleDefinition } from "@sdkwork/deployments-pc-commons";

/**
 * Console surface of the app template marketplace. Both entries render custom
 * pages (`MarketplacePage` / `MyTemplatesPage`) rather than registry tables:
 * the storefront needs category/pricing facets and an acquire flow, and the
 * author workbench needs version + submission actions the generic table does
 * not express.
 */
export const deploymentsModule = {
  id: "marketplace",
  label: "marketplace",
  surface: "app-console",
  entries: [
    { resource: "marketplace", label: "Marketplace", description: "Browse and acquire published app templates", permission: "deploy.marketplaceTemplates.read", order: 5 },
    { resource: "myTemplates", label: "My templates", description: "Publish your apps as templates and manage versions", permission: "deploy.appTemplates.read", order: 6 },
  ],
} as const satisfies DeploymentsPcModuleDefinition;
