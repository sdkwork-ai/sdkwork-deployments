import {
  createClient as createDeployClient,
  type CertificateRenewalResponse,
  type CertificateResponse,
  type CloudAccountRegistrationResponse,
  type CloudAccountResponse,
  type CreateCertificateRequest,
  type CreateCloudAccountRequest,
  type CreateDomainHostnameRequest,
  type CreateDomainZoneRequest,
  type DomainHostnameClaimResponse,
  type DomainHostnameResponse,
  type DomainVerifyResponse,
  type DomainZoneResponse,
  type EnsureDomainHostnameClaimsRequest,
  type PageInfo,
  type SdkworkDeployAppClient,
  type UpdateDomainHostnameRequest,
  type UpdateDomainZoneRequest,
} from "@sdkwork/deployments-app-sdk";
import { createDriveAppClient, type SdkworkDriveAppClient } from "@sdkwork/drive-app-sdk";
import {
  DEPLOY_ARTIFACT_UPLOAD,
  normalizeDeploymentsPage,
  type DeploymentsAction,
  type DeploymentsActionContext,
  type DeploymentsDataSource,
  type DeploymentsRegistry,
} from "@sdkwork/deployments-pc-commons";
import type { AuthTokenManager } from "@sdkwork/sdk-common";
import { uuid } from "@sdkwork/utils/id";
import { createContext, useContext, useMemo, type ReactNode } from "react";

export type {
  CertificateRenewalResponse,
  CertificateResponse,
  CloudAccountRegistrationResponse,
  CloudAccountResponse,
  CreateCertificateRequest,
  CreateCloudAccountRequest,
  CreateDomainHostnameRequest,
  CreateDomainZoneRequest,
  DomainHostnameClaimResponse,
  DomainHostnameResponse,
  DomainVerifyResponse,
  DomainZoneResponse,
  EnsureDomainHostnameClaimsRequest,
  PageInfo,
  UpdateDomainHostnameRequest,
  UpdateDomainZoneRequest,
};

export interface DeploymentsConsoleClients {
  deploy: SdkworkDeployAppClient;
  drive: SdkworkDriveAppClient;
}

export interface DeploymentsDeliveryService {
  listDomainZones(params?: {
    page?: number | undefined
    pageSize?: number | undefined
    status?: ("ACTIVE" | "PAUSED") | undefined
    keyword?: string | undefined
    /**
     * Ownership filter. `USER` keeps the list to the root domains the operator
     * defined; `PLATFORM` would surface the tenant-level `app.<suffix>` zones
     * the deployment provisions for app publishing. Those are subdomains, not
     * root domains, so a root-domain list asks for `USER` — their subdomains
     * are reached by opening the root domain, never by listing them here.
     */
    scope?: ("USER" | "PLATFORM") | undefined
  }): Promise<{ items: DomainZoneResponse[]; pageInfo: PageInfo }>;
  createDomainZone(body: CreateDomainZoneRequest): Promise<DomainZoneResponse>;
  retrieveDomainZone(zoneId: string): Promise<DomainZoneResponse>;
  updateDomainZone(zoneId: string, body: UpdateDomainZoneRequest): Promise<DomainZoneResponse>;
  deleteDomainZone(zoneId: string): Promise<void>;
  listDomainHostnames(zoneId: string, params?: { page?: number; pageSize?: number }): Promise<{ items: DomainHostnameResponse[]; pageInfo: PageInfo }>;
  createDomainHostname(zoneId: string, body: CreateDomainHostnameRequest): Promise<DomainHostnameResponse>;
  updateDomainHostname(zoneId: string, hostnameId: string, body: UpdateDomainHostnameRequest): Promise<DomainHostnameResponse>;
  verifyDomainHostname(zoneId: string, hostnameId: string): Promise<DomainVerifyResponse>;
  ensureDomainHostnameClaims(zoneId: string, body: EnsureDomainHostnameClaimsRequest): Promise<{ items: DomainHostnameClaimResponse[] }>;
  deleteDomainHostname(zoneId: string, hostnameId: string): Promise<void>;
  listCertificates(params?: { page?: number; pageSize?: number }): Promise<{ items: CertificateResponse[]; pageInfo: PageInfo }>;
  createCertificate(body: CreateCertificateRequest): Promise<CertificateResponse>;
  renewCertificate(certificateId: string): Promise<CertificateResponse>;
  /** The renewal ledger for one certificate: every attempt, oldest first. */
  listCertificateRenewals(certificateId: string, params?: { page?: number; pageSize?: number }): Promise<{ items: CertificateRenewalResponse[]; pageInfo: PageInfo }>;
  deleteCertificate(certificateId: string): Promise<void>;
  /**
   * Cloud accounts the caller may bind a root domain or certificate to.
   *
   * Ordered narrowest scope first — the caller's own accounts, then the tenant's
   * shared ones, then platform ones — which is the order the server itself
   * resolves in. `items[0]` is therefore the account a registration would reuse.
   */
  listCloudAccounts(params?: {
    page?: number | undefined
    pageSize?: number | undefined
    dnsProvider?: CloudAccountDnsProvider | undefined
    scopeType?: CloudAccountScopeType | undefined
    mine?: boolean | undefined
    keyword?: string | undefined
  }): Promise<{ items: CloudAccountResponse[]; pageInfo: PageInfo }>;
  /**
   * Registers a DNS cloud account, or reuses the caller's existing one.
   *
   * `reused` distinguishes "已创建" from "已存在，直接复用"; `credentialApplied` is
   * `false` when an existing account was found with a credential already, because
   * silently rotating one another module may be using is not this call's decision.
   */
  createCloudAccount(body: CreateCloudAccountRequest): Promise<CloudAccountRegistrationResponse>;
}

/**
 * Canonical DNS families the account center can be asked for.
 *
 * Derived from the response DTO rather than the generated `*ListParams` interface:
 * the generator exports API classes only, so param interfaces are not part of the
 * package's public surface (every other list method here inlines its params too).
 * The response is the contract both sides agree on, so deriving from it keeps the
 * picker's vocabulary pinned to the server's.
 */
export type CloudAccountDnsProvider = NonNullable<CloudAccountResponse["dnsProvider"]>;
/** Visibility levels the picker offers. `organization` is deliberately absent. */
export type CloudAccountScopeType = CloudAccountResponse["scopeType"];

const Context = createContext<DeploymentsConsoleClients | null>(null);

export function createDeploymentsConsoleClients(config: {
  deployBaseUrl: string;
  driveBaseUrl: string;
  tokenManager: AuthTokenManager;
}): DeploymentsConsoleClients {
  const common = {
    authMode: "dual-token" as const,
    platform: "pc",
    tokenManager: config.tokenManager,
  };
  return {
    deploy: createDeployClient({ ...common, baseUrl: config.deployBaseUrl }),
    drive: createDriveAppClient({ ...common, baseUrl: config.driveBaseUrl }),
  };
}

export function DeploymentsConsoleProvider({ children, clients }: { children: ReactNode; clients: DeploymentsConsoleClients }) {
  return <Context.Provider value={clients}>{children}</Context.Provider>;
}

export function useDeploymentsConsoleClients(): DeploymentsConsoleClients {
  const value = useContext(Context);
  if (!value) throw new Error("DeploymentsConsoleProvider is required");
  return value;
}

export function createDeploymentsDeliveryService(client: SdkworkDeployAppClient): DeploymentsDeliveryService {
  const zones = client.domain.domainZones;
  return {
    listDomainZones: (params) =>
      zones.list(
        params && {
          ...(params.page === undefined ? {} : { page: params.page }),
          ...(params.pageSize === undefined ? {} : { pageSize: params.pageSize }),
          ...(params.status === undefined ? {} : { status: params.status }),
          ...(params.keyword === undefined ? {} : { keyword: params.keyword }),
          ...(params.scope === undefined ? {} : { scope: params.scope }),
        },
      ),
    createDomainZone: (body) => zones.create(body, idempotencyParams()),
    retrieveDomainZone: (zoneId) => zones.retrieve(zoneId),
    updateDomainZone: (zoneId, body) => zones.update(zoneId, body),
    deleteDomainZone: (zoneId) => zones.delete(zoneId),
    listDomainHostnames: (zoneId, params) => zones.hostnames.list(zoneId, params),
    createDomainHostname: (zoneId, body) => zones.hostnames.create(zoneId, body, idempotencyParams()),
    updateDomainHostname: (zoneId, hostnameId, body) => zones.hostnames.update(zoneId, hostnameId, body),
    verifyDomainHostname: (zoneId, hostnameId) => zones.hostnames.verify(zoneId, hostnameId, idempotencyParams()),
    ensureDomainHostnameClaims: (zoneId, body) => zones.hostnameClaims.ensure(zoneId, body, idempotencyParams()),
    deleteDomainHostname: (zoneId, hostnameId) => zones.hostnames.delete(zoneId, hostnameId),
    listCertificates: (params) => client.certificate.list(params),
    createCertificate: (body) => client.certificate.create(body, idempotencyParams()),
    renewCertificate: (certificateId) => client.certificate.renew(certificateId, idempotencyParams()),
    listCertificateRenewals: (certificateId, params) => client.certificate.renewals.list(certificateId, params),
    deleteCertificate: (certificateId) => client.certificate.delete(certificateId),
    // Generated query params declare their optionals as `field?: T`, so an
    // explicit `undefined` is rejected under `exactOptionalPropertyTypes`; the
    // spread keeps unset members out of the request entirely.
    listCloudAccounts: (params) =>
      client.domain.cloudAccounts.list(
        params && {
          ...(params.page === undefined ? {} : { page: params.page }),
          ...(params.pageSize === undefined ? {} : { pageSize: params.pageSize }),
          ...(params.dnsProvider === undefined ? {} : { dnsProvider: params.dnsProvider }),
          ...(params.scopeType === undefined ? {} : { scopeType: params.scopeType }),
          ...(params.mine === undefined ? {} : { mine: params.mine }),
          ...(params.keyword === undefined ? {} : { keyword: params.keyword }),
        },
      ),
    createCloudAccount: (body) =>
      client.domain.cloudAccounts.create(body, idempotencyParams()),
  };
}

export function useDeploymentsDeliveryService(): DeploymentsDeliveryService {
  const { deploy } = useDeploymentsConsoleClients();
  return useMemo(() => createDeploymentsDeliveryService(deploy), [deploy]);
}

export function createDeploymentsConsoleRegistry(clients: DeploymentsConsoleClients): DeploymentsRegistry {
  const client = clients.deploy;
  return {
    apps: source(
      (query) => client.app.list({
        page: query.page,
        pageSize: query.pageSize,
        // Generated request params keep `keyword?: string`; unset members are
        // omitted instead of being passed as `undefined`
        // (exactOptionalPropertyTypes forbids the explicit undefined).
        ...(query.search === undefined ? {} : { keyword: query.search }),
      }),
      [
        action("create", "Create application", { name: "", slug: "", description: "", appKind: "WEB" }, (context) =>
          client.app.create(
            context.body as unknown as Parameters<typeof client.app.create>[0],
            idempotencyParams(),
          )),
        action("update", "Update", { name: "", description: "" }, (context) =>
          client.app.update(selected(context, "id"), context.body as unknown as Parameters<typeof client.app.update>[1]), { selection: true }),
        action("activate", "Activate", { appStatus: "ACTIVE" }, (context) =>
          client.app.update(selected(context, "id"), context.body as unknown as Parameters<typeof client.app.update>[1]), { selection: true }),
        action("pause", "Disable", { appStatus: "PAUSED" }, (context) =>
          client.app.update(selected(context, "id"), context.body as unknown as Parameters<typeof client.app.update>[1]), { dangerous: true, selection: true }),
        // Retirement is an archive transition, not a hard delete: the app-API
        // exposes `PATCH /apps/{appId}` with `AppStatus.ARCHIVED` (§ applications
        // convergence) and deliberately has no `DELETE /apps/{appId}`.
        action("delete", "Archive", { appStatus: "ARCHIVED" }, (context) =>
          client.app.update(selected(context, "id"), context.body as unknown as Parameters<typeof client.app.update>[1]), { dangerous: true, selection: true }),
      ],
    ),
    configuration: scoped(
      (query) => client.envVariable.apps.envVariables.list(requiredAppId(query.scopeId), {
        ...(query.search === undefined ? {} : { environment: query.search }),
      }),
      [
        action("variable", "Add variable", { key: "", value: "", environment: "production", isSecret: false }, (context) =>
          client.envVariable.apps.envVariables.create(
            requiredAppId(context.scopeId),
            context.body as unknown as Parameters<typeof client.envVariable.apps.envVariables.create>[1],
            idempotencyParams(),
          ), { scope: true }),
        action("check", "Add health check", { name: "", url: "", checkInterval: 30 }, (context) =>
          client.monitor.apps.healthChecks.create(
            requiredAppId(context.scopeId),
            context.body as unknown as Parameters<typeof client.monitor.apps.healthChecks.create>[1],
            idempotencyParams(),
          ), { scope: true }),
      ],
    ),
    domains: source(
      (query) => client.domain.domainZones.list({
        page: query.page,
        pageSize: query.pageSize,
        ...(query.search === undefined ? {} : { keyword: query.search }),
      }),
      [],
    ),
    certificates: source((query) => client.certificate.list({ page: query.page, pageSize: query.pageSize }), []),
    artifacts: source(
      (query) => client.artifact.list({ page: query.page, pageSize: query.pageSize }),
      [
        action("upload", "Upload application", { packageType: 1, checksumSha256: "" }, async (context) => {
          const file = context.file;
          const appId = requiredAppId(context.scopeId);
          if (!file) throw new Error("Package file is required");
          const idempotencyKey = uuid();
          const uploaded = await clients.drive.uploader.uploadArchive({
            file,
            // Upload identity comes from the application upload declaration
            // (`DRIVE_SPEC.md` §18); do not inline these values here.
            appResourceType: DEPLOY_ARTIFACT_UPLOAD.appResourceType,
            appResourceId: appId,
            scene: DEPLOY_ARTIFACT_UPLOAD.scene,
            source: DEPLOY_ARTIFACT_UPLOAD.source,
            originalFileName: file.name,
            contentType: file.type || "application/octet-stream",
          });
          // The uploader's checksum is optional on the Drive side, so omit the
          // member entirely when neither source produced one.
          const checksumSha256 = stringValue(context.body.checksumSha256) || uploaded.uploadItem.checksumSha256Hex;
          return client.artifact.create({
            // `CreateArtifactRequest.siteId` carries the owning application id; the
            // backend DTO has not been renamed yet (see § applications convergence).
            siteId: appId,
            packageType: Number(context.body.packageType ?? 1),
            fileName: file.name,
            contentType: file.type || "application/octet-stream",
            contentLength: String(file.size),
            ...(checksumSha256 === undefined ? {} : { checksumSha256 }),
            driveUploadSessionId: uploaded.uploadSession.id,
            driveUploadItemId: uploaded.uploadItem.id,
            driveSpaceId: uploaded.uploadItem.spaceId,
            driveNodeId: uploaded.uploadItem.nodeId,
            idempotencyKey,
          }, { idempotencyKey });
        }, { file: true, scope: true }),
        action("delete", "Retain and remove", {}, (context) => client.artifact.delete(selected(context, "id")), { dangerous: true, selection: true }),
      ],
    ),
    releases: scoped(
      (query) => client.release.list(requiredAppId(query.scopeId), { page: query.page, pageSize: query.pageSize }),
      [action("create", "Create release", { artifactId: "", versionTag: "" }, (context) => {
        const idempotencyKey = uuid();
        return client.release.create(
          requiredAppId(context.scopeId),
          { ...context.body, idempotencyKey } as unknown as Parameters<typeof client.release.create>[1],
          { idempotencyKey },
        );
      }, { scope: true })],
    ),
    deployments: scoped(
      (query) => client.deployment.list(requiredAppId(query.scopeId), { page: query.page, pageSize: query.pageSize }),
      [
        action("deploy", "Start deployment", { deploymentKind: "FULL", deploymentTarget: "CLOUD", releaseId: "", platformTargetId: "", environment: "production" }, (context) => {
          const idempotencyKey = uuid();
          return client.deployment.create(
            requiredAppId(context.scopeId),
            { ...context.body, idempotencyKey } as unknown as Parameters<typeof client.deployment.create>[1],
            { idempotencyKey },
          );
        }, { scope: true }),
        // Rollback is a forward fix, not an in-place restore: the app-API records
        // lineage in `rollbackFromDeploymentId` (response-only) and there is no
        // `deployments.rollback` operation. Redeploying the previous release's
        // artifact produces a new deployment that carries the rollback lineage.
        action("rollback", "Rollback", {}, (context) =>
          client.deployment.create(
            requiredAppId(context.scopeId),
            {
              platformTargetId: String(context.selectedItem?.platformTargetId ?? ""),
              releaseId: String(context.selectedItem?.rollbackReleaseId ?? context.selectedItem?.releaseId ?? ""),
              deploymentKind: "FULL",
              deploymentTarget: "CLOUD",
              idempotencyKey: uuid(),
            } as unknown as Parameters<typeof client.deployment.create>[1],
            idempotencyParams(),
          ), { dangerous: true, scope: true, selection: true }),
      ],
    ),
    monitoring: scoped(
      (query) => client.monitor.apps.healthChecks.list(requiredAppId(query.scopeId)),
      [action("create", "Add health check", { name: "", url: "", checkInterval: 30 }, (context) =>
        client.monitor.apps.healthChecks.create(
          requiredAppId(context.scopeId),
          context.body as unknown as Parameters<typeof client.monitor.apps.healthChecks.create>[1],
          idempotencyParams(),
        ), { scope: true })],
    ),
  };
}

function source(
  load: (query: Parameters<DeploymentsDataSource["load"]>[0]) => Promise<unknown>,
  actions: readonly DeploymentsAction[],
): DeploymentsDataSource {
  return {
    actions,
    async load(query) {
      return normalizeDeploymentsPage(await load(query));
    },
  };
}

function scoped(load: Parameters<typeof source>[0], actions: readonly DeploymentsAction[]): DeploymentsDataSource {
  return { ...source(load, actions), requiresScope: true };
}

function action(
  id: string,
  label: string,
  bodyTemplate: Record<string, unknown>,
  execute: DeploymentsAction["execute"],
  options: { dangerous?: boolean; file?: boolean; scope?: boolean; selection?: boolean } = {},
): DeploymentsAction {
  return {
    id,
    label,
    bodyTemplate,
    execute,
    dangerous: options.dangerous,
    requiresFile: options.file,
    requiresScope: options.scope,
    requiresSelection: options.selection,
  };
}

// The scope id is the `deploy_app` application id: the `deploy_site` surface was
// retired in favour of the `apps` surface (see § applications convergence).
function requiredAppId(value: string | undefined): string {
  if (!value?.trim()) throw new Error("Application ID is required");
  return value.trim();
}

function selected(context: DeploymentsActionContext, field: string): string {
  const value = context.selectedItem?.[field];
  if (typeof value !== "string" && typeof value !== "number") throw new Error(`${field} is unavailable`);
  return String(value);
}

function stringValue(value: unknown): string | undefined {
  return typeof value === "string" && value.trim() ? value.trim() : undefined;
}

function idempotencyParams(): { idempotencyKey: string } {
  return { idempotencyKey: uuid() };
}
