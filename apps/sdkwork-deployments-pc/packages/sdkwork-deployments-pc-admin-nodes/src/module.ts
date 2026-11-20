import type { DeploymentsPcModuleDefinition } from "@sdkwork/deployments-pc-commons";
export const deploymentsModule = { id: "nodes", label: "nodes", surface: "backend-admin", entries: [
    { resource: "clusters", label: "Clusters", description: "Node cluster of host nodes", permission: "deploy.clusters.read", order: 1 },
    { resource: "nodes", label: "Nodes", description: "Host node inventory", permission: "deploy.servers.read", order: 2 }
] } as const satisfies DeploymentsPcModuleDefinition;
