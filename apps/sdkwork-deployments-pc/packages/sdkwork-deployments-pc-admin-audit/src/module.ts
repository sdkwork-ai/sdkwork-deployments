import type { DeploymentsPcModuleDefinition } from "@sdkwork/deployments-pc-commons";
export const deploymentsModule = { id: "audit", label: "audit", surface: "backend-admin", entries: [
    { resource: "audit", label: "Audit", description: "Publishing operator evidence", permission: "deploy.auditLogs.read", order: 1 }
] } as const satisfies DeploymentsPcModuleDefinition;
