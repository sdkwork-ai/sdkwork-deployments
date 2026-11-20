import type { DeploymentsPcModuleDefinition } from "@sdkwork/deployments-pc-commons";
export const deploymentsModule = { id: "infrastructure", label: "infrastructure", surface: "backend-admin", entries: [
    { resource: "nginx", label: "Nginx", description: "Publishing gateway configuration", permission: "deploy.nginx.write", order: 1 }
] } as const satisfies DeploymentsPcModuleDefinition;
