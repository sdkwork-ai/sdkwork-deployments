import type { ComponentType } from "react";

export type DeploymentsSurface = "app-console" | "backend-admin";
/**
 * The resource ids a module may contribute on either surface.
 *
 * A **closed** union on purpose: the admin shell renders one route and one icon
 * per member, and the catalog needs a `resource.<key>.label` for each, so an
 * unknown id has nothing to render with. Adding a member is therefore a
 * deliberate multi-site change (this union, {@link resourceIcon}, both language
 * catalogs, and the module that contributes it) rather than something a feature
 * package can do on its own — which is what keeps a typo'd resource id from
 * producing a blank admin route.
 *
 * `sourceSpecs` is the operation-plane counterpart of the console's per-app
 * "source specs" drawer: same rows, read across applications instead of within
 * one. The template-marketplace ids split the capability across both surfaces:
 * the admin moderates `templateCategories` / `appTemplates` / `templatePurchases`,
 * the console browses `marketplace` and manages `myTemplates`.
 */
export type DeploymentsResourceKey = "configuration" | "domains" | "certificates" | "apps" | "artifacts" | "releases" | "deployments" | "monitoring" | "nginx" | "clusters" | "nodes" | "audit" | "localProjects" | "sourceSpecs" | "templateCategories" | "appTemplates" | "templatePurchases" | "marketplace" | "myTemplates";
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
