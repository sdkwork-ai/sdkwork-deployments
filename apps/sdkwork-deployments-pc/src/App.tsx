import { useSdkworkAuthControllerState } from "@sdkwork/auth-pc-react";
import { deploymentsModule as adminAudit } from "@sdkwork/deployments-pc-admin-audit";
import { deploymentsModule as infrastructure } from "@sdkwork/deployments-pc-admin-infrastructure";
import {
  deploymentsModule as localProjects,
  LocalProjectsPage,
} from "@sdkwork/deployments-pc-admin-local-projects";
import { deploymentsModule as adminNodes } from "@sdkwork/deployments-pc-admin-nodes";
import type { DeploymentsLocale, DeploymentsPcModuleDefinition } from "@sdkwork/deployments-pc-commons";
import { createDeploymentsConsoleRegistry, DeploymentsConsoleProvider, useDeploymentsConsoleClients } from "@sdkwork/deployments-pc-console-core";
import { deploymentsModule as delivery } from "@sdkwork/deployments-pc-console-delivery";
import { deploymentsModule as monitoring } from "@sdkwork/deployments-pc-console-monitoring";
import { deploymentsModule as publishing, PublishingAppsPage } from "@sdkwork/deployments-pc-console-publishing";
import { DeploymentsConsoleShell } from "@sdkwork/deployments-pc-console-shell";
import { createDriveSandboxExplorerSdkPort } from "@sdkwork/drive-pc-sandbox-explorer-sdk-adapter";
import { lazy, Suspense, useMemo } from "react";
import { BrowserRouter, Navigate, Route, Routes } from "react-router-dom";

import { DeploymentsAuthGate } from "./auth/DeploymentsAuthGate.tsx";
import type { BootstrappedDeploymentsRuntime } from "./bootstrap/runtime.ts";

const consoleModules = [publishing, delivery, monitoring] satisfies readonly DeploymentsPcModuleDefinition[];
const LazyDomainManagementPage = lazy(() => import("@sdkwork/deployments-pc-console-delivery/management").then((module) => ({ default: module.DomainManagementPage })));
const LazyCertificateManagementPage = lazy(() => import("@sdkwork/deployments-pc-console-delivery/management").then((module) => ({ default: module.CertificateManagementPage })));

/**
 * Console bridge for the reusable publishing page: the publishing package stays
 * host-agnostic (clients arrive as props), and this app-level adapter pulls the
 * console clients from the provider so the page can mount in the workspace.
 */
function PublishingAppsBridge({ locale }: { locale: DeploymentsLocale }) {
  const { deploy, drive } = useDeploymentsConsoleClients();
  return <PublishingAppsPage deployClient={deploy} driveClient={drive} locale={locale} />;
}

const consoleResourcePages = { apps: PublishingAppsBridge, domains: LazyDomainManagementPage, certificates: LazyCertificateManagementPage } as const;
const adminModules = [localProjects, infrastructure, adminNodes, adminAudit] satisfies readonly DeploymentsPcModuleDefinition[];
const adminResourcePages = { localProjects: LocalProjectsPage } as const;
const LazyAuth = lazy(() => import("./auth/DeploymentsAuthRoutes.tsx").then((module) => ({ default: module.DeploymentsAuthRoutes })));
const LazyAdmin = lazy(() => import("./surfaces/DeploymentsAdminSurface.tsx").then((module) => ({ default: module.DeploymentsAdminSurface })));

export function App({ runtime }: { runtime: BootstrappedDeploymentsRuntime }) {
  return (
    <BrowserRouter>
      <Authenticated runtime={runtime} />
    </BrowserRouter>
  );
}

function Authenticated({ runtime }: { runtime: BootstrappedDeploymentsRuntime }) {
  const state = useSdkworkAuthControllerState(runtime.authController);
  const registry = useMemo(() => createDeploymentsConsoleRegistry(runtime.clients), [runtime.clients]);
  const sandboxExplorerPort = useMemo(
    () => createDriveSandboxExplorerSdkPort({ client: runtime.clients.drive }),
    [runtime.clients.drive],
  );
  const permissionScope = state.session?.context?.permissionScope ?? [];
  const userLabel = state.user?.displayName || state.user?.email;
  const signOut = () => {
    void runtime.authController.signOut();
  };
  return (
    <DeploymentsAuthGate
      controller={runtime.authController}
      authRoutes={
        <Suspense fallback={<div className="bootstrap-state">SDKWork Deployments</div>}>
          <LazyAuth controller={runtime.authController} />
        </Suspense>
      }
    >
      <DeploymentsConsoleProvider clients={runtime.clients}>
        <Routes>
          <Route
            path="/console/*"
            element={
              <DeploymentsConsoleShell
                locale={runtime.locale}
                modules={consoleModules}
                permissionScope={permissionScope}
                registry={registry}
                resourcePages={consoleResourcePages}
                userLabel={userLabel}
                onSignOut={signOut}
              />
            }
          />
          <Route
            path="/admin/*"
            element={
              <Suspense fallback={<div className="bootstrap-state">SDKWork Deployments</div>}>
                <LazyAdmin
                  backendApiBaseUrl={runtime.config.backendApiBaseUrl}
                  locale={runtime.locale}
                  modules={adminModules}
                  permissionScope={permissionScope}
                  resourcePages={adminResourcePages}
                  sandboxExplorerPort={sandboxExplorerPort}
                  tokenManager={runtime.tokenManager}
                  userLabel={userLabel}
                  onSignOut={signOut}
                />
              </Suspense>
            }
          />
          <Route path="*" element={<Navigate to="/console" replace />} />
        </Routes>
      </DeploymentsConsoleProvider>
    </DeploymentsAuthGate>
  );
}
