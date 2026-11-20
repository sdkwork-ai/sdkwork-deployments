import type { DeploymentsPcModuleDefinition } from "@sdkwork/deployments-pc-commons";
export const deploymentsModule = { id: "downloads", label: "downloads", surface: "backend-admin",
  // Backend-admin surface for the download catalog.
  // The page is custom rather than a registry table: a catalog contains cards, a card
  // contains installers, and every installer owns one update configuration, which
  // is three nesting levels the generic table does not model.
entries: [
    { resource: "downloadCatalog", label: "Download catalog", description: "Download page cards, installers and per-installer update feeds", permission: "deploy.downloads.read", order: 9 }
] } as const satisfies DeploymentsPcModuleDefinition;
