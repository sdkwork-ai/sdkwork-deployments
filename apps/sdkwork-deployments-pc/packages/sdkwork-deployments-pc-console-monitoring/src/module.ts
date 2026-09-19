import type { DeploymentsPcModuleDefinition } from "@sdkwork/deployments-pc-commons";
export const deploymentsModule = { id: "monitoring", label: "monitoring", surface: "app-console", entries: [
    { resource: "monitoring", label: "Monitoring", description: "Health check policy and status", permission: "deploy.healthChecks.read", order: 1 },
    { resource: "configuration", label: "Configuration", description: "Environment variables and health checks", permission: "deploy.apps.read", order: 2 }
] } as const satisfies DeploymentsPcModuleDefinition;
