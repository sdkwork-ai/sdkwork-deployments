import {
  createClient as createDeployClient,
  type AppTemplateScreenshot,
  type CreateAppTemplateRequest,
  type SdkworkDeployAppClient,
  type UpdateAppTemplateRequest,
} from "@sdkwork/deployments-app-sdk";
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
const AppClientContext = createContext<SdkworkDeployAppClient | null>(null);

export function createDeploymentsAdminClient(baseUrl: string, tokenManager: AuthTokenManager) {
  return createClient({ baseUrl, authMode: "dual-token", platform: "pc", tokenManager });
}

/**
 * App-plane client hosted by the admin surface.
 *
 * `deploy_app` application authoring and app-template listing create/edit exist
 * on the app API only: the backend-admin contract moderates a listing but has
 * no create command and no editable-copy patch. The admin surface therefore
 * keeps the app-plane client behind its `backend-admin` boundary — the same
 * split `sourceSpecsPort` already uses — and routes only those commands through
 * it (see `createDeploymentsAdminRegistry`).
 */
export function createDeploymentsAppClient(appApiBaseUrl: string, tokenManager: AuthTokenManager) {
  return createDeployClient({ baseUrl: appApiBaseUrl, authMode: "dual-token", platform: "pc", tokenManager });
}

export function DeploymentsAdminProvider({ appClient, children, client }: { appClient: SdkworkDeployAppClient; children: ReactNode; client: SdkworkDeployBackendClient }) {
  return (
    <Context.Provider value={client}>
      <AppClientContext.Provider value={appClient}>{children}</AppClientContext.Provider>
    </Context.Provider>
  );
}

export function useDeploymentsAdminClient() {
  const value = useContext(Context);
  if (!value) throw new Error("DeploymentsAdminProvider is required");
  return value;
}

export function useDeploymentsAdminAppClient() {
  const value = useContext(AppClientContext);
  if (!value) throw new Error("DeploymentsAdminProvider is required");
  return value;
}

/**
 * Builds the admin registry.
 *
 * `appClient` is optional on purpose: a host that only bridges the catalog
 * through the backend-admin client (the Web Server admin surface does) must keep
 * compiling and get the moderation-only action set, while this application's
 * admin surface injects the app-plane client to expose listing create/edit.
 */
export function createDeploymentsAdminRegistry(client: SdkworkDeployBackendClient, appClient?: SdkworkDeployAppClient): DeploymentsRegistry {
  const app = appClient;
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
        // Authoring has no backend-admin counterpart: creating a listing and
        // patching its editable copy are app-plane commands
        // (`template.appTemplates.*`), so they use the injected app client while
        // every moderation transition below stays on the backend-admin client.
        // A host that does not inject the app client keeps the moderation-only
        // action set instead of failing to compose the registry.
        // The payload construction mirrors `createMyTemplatesService()` in
        // `@sdkwork/deployments-pc-console-marketplace`.
        ...(app === undefined ? [] : [
          action("create", "Create listing", {
            appUuid: "",
            templateType: "APP",
            categoryUuid: "",
            templateKey: "",
            displayName: "",
            summary: "",
            description: "",
            visibility: "PUBLIC",
            pricingModel: "FREE",
            priceMinor: "",
            currency: "",
            iconMediaRef: "",
            coverMediaRef: "",
            screenshots: "",
          }, async (context) =>
            app.template.appTemplates.create(appTemplateCreateBody(context.body), idempotencyParams())),
          action("update", "Update listing", {
            displayName: "",
            summary: "",
            description: "",
            categoryUuid: "",
            visibility: "",
            pricingModel: "",
            priceMinor: "",
            currency: "",
            iconMediaRef: "",
            coverMediaRef: "",
            screenshots: "",
          }, async (context) =>
            app.template.appTemplates.update(selected(context, "id"), appTemplateUpdateBody(context.body), idempotencyParams()), { selection: true }),
        ]),
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

/**
 * The create command's required members, assembled from the generic dialog's
 * flat body. Required fields fail loudly rather than sending `""` for a
 * uuid- or enum-typed member the server would reject; optional members are
 * omitted instead of being sent as `undefined` (`exactOptionalPropertyTypes`).
 */
function appTemplateCreateBody(body: Record<string, unknown>): CreateAppTemplateRequest {
  const cleaned = cleanBody(body);
  const description = optionalText(cleaned.description, false);
  const currency = optionalText(cleaned.currency, true);
  const priceMinor = optionalText(cleaned.priceMinor, true);
  const kind = templateType(cleaned.templateType);
  const access = visibility(cleaned.visibility);
  const pricing = pricingModel(cleaned.pricingModel);
  return {
    appUuid: requiredText(cleaned, "appUuid"),
    categoryUuid: requiredText(cleaned, "categoryUuid"),
    templateKey: requiredText(cleaned, "templateKey"),
    displayName: requiredText(cleaned, "displayName"),
    summary: requiredText(cleaned, "summary"),
    ...(kind === undefined ? {} : { templateType: kind }),
    ...(access === undefined ? {} : { visibility: access }),
    ...(pricing === undefined ? {} : { pricingModel: pricing }),
    ...(description === undefined ? {} : { description }),
    // A PAID listing is the only one that may carry a price; the author command
    // keeps the member absent otherwise (mirrors `createMyTemplatesService`).
    ...(pricing === "PAID" && priceMinor !== undefined ? { priceMinor } : {}),
    ...(currency === undefined ? {} : { currency: currency.toUpperCase() }),
    ...appTemplateMedia(cleaned),
  };
}

/**
 * The editable-copy patch: an omitted field keeps its stored value server-side,
 * so only the members the operator filled in are sent.
 */
function appTemplateUpdateBody(body: Record<string, unknown>): UpdateAppTemplateRequest {
  const cleaned = cleanBody(body);
  const displayName = optionalText(cleaned.displayName, true);
  const summary = optionalText(cleaned.summary, true);
  const description = optionalText(cleaned.description, false);
  const categoryUuid = optionalText(cleaned.categoryUuid, true);
  const visibilityValue = visibility(cleaned.visibility);
  const pricing = pricingModel(cleaned.pricingModel);
  const priceMinor = optionalText(cleaned.priceMinor, true);
  const currency = optionalText(cleaned.currency, true);
  return {
    ...(displayName === undefined ? {} : { displayName }),
    ...(summary === undefined ? {} : { summary }),
    ...(description === undefined ? {} : { description }),
    ...(categoryUuid === undefined ? {} : { categoryUuid }),
    ...(visibilityValue === undefined ? {} : { visibility: visibilityValue }),
    ...(pricing === undefined ? {} : { pricingModel: pricing }),
    ...(priceMinor === undefined ? {} : { priceMinor }),
    ...(currency === undefined ? {} : { currency: currency.toUpperCase() }),
    ...appTemplateMedia(cleaned),
  };
}

/**
 * Presentation media shared by the create and update commands: the icon, the
 * cover, and the screenshot gallery. The gallery is authored as a JSON array in
 * the generic dialog (`screenshots`) because this registry table is field-driven
 * and has no array editor; it is validated here so a malformed value fails with
 * a clear field error instead of reaching the API as an opaque string.
 */
function appTemplateMedia(
  body: Record<string, unknown>,
): Pick<UpdateAppTemplateRequest, "iconMediaRef" | "coverMediaRef" | "screenshots"> {
  const iconMediaRef = optionalText(body.iconMediaRef, true);
  const coverMediaRef = optionalText(body.coverMediaRef, true);
  const screenshots = parseScreenshots(body.screenshots);
  return {
    ...(iconMediaRef === undefined ? {} : { iconMediaRef }),
    ...(coverMediaRef === undefined ? {} : { coverMediaRef }),
    ...(screenshots === undefined ? {} : { screenshots }),
  };
}

function parseScreenshots(value: unknown): AppTemplateScreenshot[] | undefined {
  const text = optionalText(value, false);
  if (text === undefined) {
    return undefined;
  }
  let parsed: unknown;
  try {
    parsed = JSON.parse(text);
  } catch {
    throw new Error("screenshots must be a JSON array");
  }
  if (!Array.isArray(parsed) || parsed.length > 8) {
    throw new Error("screenshots must be a JSON array of at most 8 entries");
  }
  return parsed.map((entry, index) => {
    const record = typeof entry === "object" && entry !== null ? (entry as Record<string, unknown>) : undefined;
    if (record === undefined) {
      throw new Error(`screenshots[${index}] must be an object`);
    }
    const mediaRef = optionalText(record.mediaRef, true);
    if (mediaRef === undefined) {
      throw new Error(`screenshots[${index}].mediaRef is required`);
    }
    const device = optionalText(record.device, true);
    const caption = optionalText(record.caption, false);
    const sortOrder = typeof record.sortOrder === "number" && Number.isInteger(record.sortOrder) && record.sortOrder >= 0
      ? record.sortOrder
      : undefined;
    return {
      mediaRef,
      ...(device === undefined ? {} : { device }),
      ...(caption === undefined ? {} : { caption }),
      ...(sortOrder === undefined ? {} : { sortOrder }),
    };
  });
}

function requiredText(body: Record<string, unknown>, field: string): string {
  const value = body[field];
  if (typeof value !== "string" || value.trim() === "") throw new Error(`${field} is required`);
  return value.trim();
}

function optionalText(value: unknown, trim: boolean): string | undefined {
  if (typeof value !== "string" || value.trim() === "") return undefined;
  return trim ? value.trim() : value;
}

function templateType(value: unknown): CreateAppTemplateRequest["templateType"] {
  return value === "APP" || value === "PPT" || value === "VIDEO" ? value : undefined;
}

function visibility(value: unknown): UpdateAppTemplateRequest["visibility"] {
  return value === "PUBLIC" || value === "PRIVATE" ? value : undefined;
}

function pricingModel(value: unknown): UpdateAppTemplateRequest["pricingModel"] {
  return value === "FREE" || value === "PAID" ? value : undefined;
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
