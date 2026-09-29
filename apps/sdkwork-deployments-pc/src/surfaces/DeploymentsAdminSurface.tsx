import { createDeploymentsAdminClient, createDeploymentsAdminRegistry, DeploymentsAdminProvider, SourceSpecsAdminPortProvider, type SourceSpecsAdminPort } from "@sdkwork/deployments-pc-admin-core";
import { LocalProjectsExplorerPortProvider } from "@sdkwork/deployments-pc-admin-local-projects";
import { DeploymentsAdminShell } from "@sdkwork/deployments-pc-admin-shell";
import type {
  DeploymentsLocale,
  DeploymentsPcModuleDefinition,
  DeploymentsResourcePages,
} from "@sdkwork/deployments-pc-commons";
import type { SandboxExplorerPort } from "@sdkwork/drive-pc-sandbox-contracts";
import type { AuthTokenManager } from "@sdkwork/sdk-common";
import { useMemo } from "react";

export interface DeploymentsAdminSurfaceProps {
  backendApiBaseUrl: string;
  locale: DeploymentsLocale;
  modules: readonly DeploymentsPcModuleDefinition[];
  onSignOut(): void;
  permissionScope: readonly string[];
  resourcePages?: DeploymentsResourcePages | undefined;
  sandboxExplorerPort?: SandboxExplorerPort | null;
  /**
   * Read port for the source-specs ledger.
   *
   * Built by the composition root (the app shell), not here, for the same reason
   * `sandboxExplorerPort` is: the port is an *app-plane* projection, and this
   * component's own client is the backend-admin one, which has no source-spec
   * surface to ask. `null` is a legitimate value — the page explains it on
   * screen — so the default is `null` rather than a required prop.
   */
  sourceSpecsPort?: SourceSpecsAdminPort | null;
  tokenManager: AuthTokenManager;
  userLabel?: string | undefined;
}

export function DeploymentsAdminSurface({
  backendApiBaseUrl,
  locale,
  modules,
  onSignOut,
  permissionScope,
  resourcePages,
  sandboxExplorerPort = null,
  sourceSpecsPort = null,
  tokenManager,
  userLabel,
}: DeploymentsAdminSurfaceProps) {
  const client = useMemo(() => createDeploymentsAdminClient(backendApiBaseUrl, tokenManager), [backendApiBaseUrl, tokenManager]);
  const registry = useMemo(() => createDeploymentsAdminRegistry(client), [client]);
  return (
    <LocalProjectsExplorerPortProvider port={sandboxExplorerPort}>
      <SourceSpecsAdminPortProvider port={sourceSpecsPort}>
        <DeploymentsAdminProvider client={client}>
          <DeploymentsAdminShell
            locale={locale}
            modules={modules}
            permissionScope={permissionScope}
            registry={registry}
            resourcePages={resourcePages}
            userLabel={userLabel}
            onSignOut={onSignOut}
          />
        </DeploymentsAdminProvider>
      </SourceSpecsAdminPortProvider>
    </LocalProjectsExplorerPortProvider>
  );
}
