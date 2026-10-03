import { createClient, type SdkworkDeployBackendClient } from "@sdkwork/deployments-backend-sdk";
import {
  normalizeDeploymentsPage,
  type DeploymentsAction,
  type DeploymentsActionContext,
  type DeploymentsDataSource,
  type DeploymentsRegistry,
} from "@sdkwork/deployments-pc-commons";
import type { AuthTokenManager } from "@sdkwork/sdk-common";
import { uuid } from "@sdkwork/utils/id";
import { createContext, useContext, type ReactNode } from "react";

const Context = createContext<SdkworkDeployBackendClient | null>(null);

export function createDeploymentsAdminClient(baseUrl: string, tokenManager: AuthTokenManager) {
  return createClient({ baseUrl, authMode: "dual-token", platform: "pc", tokenManager });
}

export function DeploymentsAdminProvider({ children, client }: { children: ReactNode; client: SdkworkDeployBackendClient }) {
  return <Context.Provider value={client}>{children}</Context.Provider>;
}

export function useDeploymentsAdminClient() {
  const value = useContext(Context);
  if (!value) throw new Error("DeploymentsAdminProvider is required");
  return value;
}

export function createDeploymentsAdminRegistry(client: SdkworkDeployBackendClient): DeploymentsRegistry {
  return {
    nginx: source(
      (query) => client.nginx.configs.list({ page: query.page, pageSize: query.pageSize }),
      [
        action("create", "Create config", { name: "", content: "", description: "" }, (context) =>
          client.nginx.configs.create(
            context.body as unknown as Parameters<typeof client.nginx.configs.create>[0],
            idempotencyParams(),
          )),
        action("validate", "Validate", {}, (context) =>
          client.nginx.configs.validate(selected(context, "id"), idempotencyParams()), { selection: true }),
        action("deploy", "Deploy", {}, (context) =>
          client.nginx.configs.deploy(selected(context, "id"), idempotencyParams()), { dangerous: true, selection: true }),
        action("reload", "Reload", {}, () => client.nginx.runtime.reload(idempotencyParams()), { dangerous: true }),
      ],
      ["configName", "siteId", "configType"],
    ),
    nodes: source(
      (query) => client.server.list({ page: query.page, pageSize: query.pageSize }),
      [
        action("create", "Register node", { name: "", host: "", sshPort: 22, sshUser: "root", clusterId: "", description: "" }, (context) =>
          client.server.create(
            context.body as unknown as Parameters<typeof client.server.create>[0],
            idempotencyParams(),
          )),
        action("update", "Update node", { status: 1, clusterId: "", description: "" }, (context) =>
          client.server.update(
            selected(context, "id"),
            context.body as unknown as Parameters<typeof client.server.update>[1],
          ), { selection: true }),
      ],
      ["name", "host", "clusterName"],
    ),
    clusters: source(
      (query) => client.cluster.list({ page: query.page, pageSize: query.pageSize }),
      [
        action("create", "Create cluster", { name: "", description: "", region: "" }, (context) =>
          client.cluster.create(
            context.body as unknown as Parameters<typeof client.cluster.create>[0],
            idempotencyParams(),
          )),
        action("update", "Update cluster", { status: 1, description: "" }, (context) =>
          client.cluster.update(
            selected(context, "id"),
            context.body as unknown as Parameters<typeof client.cluster.update>[1],
          ), { selection: true }),
      ],
      ["name", "region", "description"],
    ),
    audit: source((query) => client.audit.auditLogs.list({ page: query.page, pageSize: query.pageSize }), [], ["action", "resource"]),
    templateCategories: source(
      (query) => client.template.templateCategories.list({ page: query.page, pageSize: query.pageSize, includeDisabled: true }),
      [
        action("create", "Create category", { parentId: "", categoryKey: "", displayName: "", description: "", sortOrder: 0 }, (context) =>
          client.template.templateCategories.create(
            cleanBody(context.body) as unknown as Parameters<typeof client.template.templateCategories.create>[0],
            idempotencyParams(),
          )),
        action("update", "Update category", { displayName: "", description: "", sortOrder: 0, status: "ACTIVE" }, (context) =>
          client.template.templateCategories.update(
            selected(context, "id"),
            cleanBody(context.body) as unknown as Parameters<typeof client.template.templateCategories.update>[1],
            idempotencyParams(),
          ), { selection: true }),
        // Soft delete: the baseline frees the `category_key` for a same-key
        // rebuild, so the taxonomy can be corrected without losing listings
        // that still reference a disabled parent.
        action("delete", "Delete category", {}, (context) =>
          client.template.templateCategories.delete(selected(context, "id")), { dangerous: true, selection: true }),
      ],
      ["displayName", "categoryKey"],
    ),
    appTemplates: source(
      (query) => client.template.appTemplates.list({ page: query.page, pageSize: query.pageSize }),
      [
        // Moderation moves status only; the author owns the listing copy. An
        // approval publishes the newest draft versions server-side.
        action("publish", "Approve listing", { status: "PUBLISHED" }, (context) =>
          client.template.appTemplates.update(selected(context, "id"), { status: "PUBLISHED" }, idempotencyParams()), { selection: true }),
        action("reject", "Reject listing", { status: "REJECTED", reviewNote: "" }, (context) =>
          client.template.appTemplates.update(
            selected(context, "id"),
            cleanBody(context.body) as unknown as Parameters<typeof client.template.appTemplates.update>[1],
            idempotencyParams(),
          ), { selection: true }),
        action("disable", "Disable listing", { status: "DISABLED" }, (context) =>
          client.template.appTemplates.update(selected(context, "id"), { status: "DISABLED" }, idempotencyParams()), { dangerous: true, selection: true }),
        action("feature", "Feature listing", { isFeatured: true }, (context) =>
          client.template.appTemplates.update(selected(context, "id"), { isFeatured: true }, idempotencyParams()), { selection: true }),
        action("unfeature", "Unfeature listing", { isFeatured: false }, (context) =>
          client.template.appTemplates.update(selected(context, "id"), { isFeatured: false }, idempotencyParams()), { selection: true }),
        action("delete", "Delete listing", {}, (context) =>
          client.template.appTemplates.delete(selected(context, "id")), { dangerous: true, selection: true }),
      ],
      ["displayName", "templateKey", "status"],
    ),
    // Versions are a nested collection (`/app_templates/{uuid}/versions`), so
    // this resource is scoped: the shell's scope field carries the listing's
    // template uuid and the table loads nothing until it is present.
    appTemplateVersions: source(
      (query) => client.template.appTemplateVersions.list(query.scopeId ?? "", { page: query.page, pageSize: query.pageSize }),
      [],
      ["templateVersion", "status"],
      { requiresScope: true },
    ),
    // No purchase resource: app-template trade is the platform order center's
    // (`sdkwork-order`) and the deployments app API is catalog-only, so this
    // registry has no ledger to read and no entitlement to revoke.
  };
}

function source(
  load: (query: Parameters<DeploymentsDataSource["load"]>[0]) => Promise<unknown>,
  actions: readonly DeploymentsAction[],
  searchFields: readonly string[] = [],
  options: { requiresScope?: boolean } = {},
): DeploymentsDataSource {
  return {
    actions,
    requiresScope: options.requiresScope,
    async load(query) {
      const page = normalizeDeploymentsPage(await load(query));
      const needle = query.search?.trim().toLowerCase();
      if (!needle || searchFields.length === 0) return page;
      return {
        ...page,
        items: page.items.filter((item) =>
          searchFields.some((field) => String(item[field] ?? "").toLowerCase().includes(needle))),
      };
    },
  };
}

function action(
  id: string,
  label: string,
  bodyTemplate: Record<string, unknown>,
  execute: DeploymentsAction["execute"],
  options: { dangerous?: boolean; selection?: boolean } = {},
): DeploymentsAction {
  return {
    id,
    label,
    bodyTemplate,
    execute,
    dangerous: options.dangerous,
    requiresSelection: options.selection,
  };
}

function selected(context: DeploymentsActionContext, field: string): string {
  const value = context.selectedItem?.[field] ?? context.selectedItem?.configId;
  if (typeof value !== "string" && typeof value !== "number") throw new Error(`${field} is unavailable`);
  return String(value);
}

/**
 * Drops blank string fields from a generic dialog body: the marketplace write
 * contracts treat an absent optional as "leave unchanged / not provided",
 * while `""` for a uuid- or enum-typed field is a validation error. Numbers
 * and booleans are real values and always pass through.
 */
function cleanBody(body: Record<string, unknown>): Record<string, unknown> {
  return Object.fromEntries(
    Object.entries(body).filter(([, value]) => !(typeof value === "string" && value.trim() === "")),
  );
}

function idempotencyParams(): { idempotencyKey: string } {
  return { idempotencyKey: uuid() };
}

/* ------------------------------------------------------------------ *
 * 源码规格运维视图
 * ------------------------------------------------------------------ *
 * 它自带的是**端口**而不是 SDK 客户端 —— backend-admin SDK 没有源码规格面
 * （见 `source-specs/port.tsx` 的说明），所以本包不为它多引一个依赖，由应用壳
 * （组合根）把端口喂进来。放在 admin-core 而不是新开一个包，是因为新增工作区包
 * 需要一次跨仓 `pnpm install` 才能建好 `node_modules` 链接，而本次改动不需要
 * 为它付这个代价。
 */
export { SourceSpecsAdminPage } from "./source-specs/SourceSpecsAdminPage.tsx";
export type { SourceSpecsAdminPageProps } from "./source-specs/SourceSpecsAdminPage.tsx";
export { sourceSpecsModule } from "./source-specs/module.ts";
export { SourceSpecsAdminPortProvider, useSourceSpecsAdminPort } from "./source-specs/port.tsx";
export type {
  SourceSpecsAdminApplication,
  SourceSpecsAdminPage as SourceSpecsAdminLedgerPage,
  SourceSpecsAdminPort,
  SourceSpecsAdminRoute,
  SourceSpecsAdminSpec,
} from "./source-specs/port.tsx";
