import type { ComponentType } from "react";

export type DeploymentsSurface = "app-console" | "backend-admin";
export type DeploymentsResourceKey = "configuration" | "domains" | "certificates" | "apps" | "artifacts" | "releases" | "deployments" | "monitoring" | "nginx" | "clusters" | "nodes" | "audit" | "localProjects";
export interface DeploymentsModuleEntry { description: string; label: string; order: number; permission?: string | undefined; resource: DeploymentsResourceKey; }
export interface DeploymentsPcModuleDefinition { entries: readonly DeploymentsModuleEntry[]; id: string; label: string; surface: DeploymentsSurface; }
export interface DeploymentsQuery { page: number; pageSize: number; scopeId?: string | undefined; search?: string | undefined; }
export interface DeploymentsPage { items: readonly Record<string, unknown>[]; pageInfo: { page: number; pageSize: number; hasMore: boolean; total?: number | undefined }; }
export interface DeploymentsActionContext { body: Record<string, unknown>; file?: File | undefined; scopeId?: string | undefined; selectedItem?: Record<string, unknown> | undefined; }
export interface DeploymentsAction { bodyTemplate: Record<string, unknown>; dangerous?: boolean | undefined; execute(context: DeploymentsActionContext): Promise<unknown>; id: string; label: string; requiresFile?: boolean | undefined; requiresScope?: boolean | undefined; requiresSelection?: boolean | undefined; }
export interface DeploymentsDataSource { actions: readonly DeploymentsAction[]; load(query: DeploymentsQuery): Promise<DeploymentsPage>; requiresScope?: boolean | undefined; }
export type DeploymentsRegistry = Partial<Record<DeploymentsResourceKey, DeploymentsDataSource>>;
export interface DeploymentsResourcePageProps { locale: import("./i18n/index.ts").DeploymentsLocale; }
export type DeploymentsResourcePages = Partial<Record<DeploymentsResourceKey, ComponentType<DeploymentsResourcePageProps>>>;
