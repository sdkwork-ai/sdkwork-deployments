import { createSdkworkIamRuntimeAuthController, type SdkworkIamRuntimeAuthRuntimeLike } from "@sdkwork/auth-pc-react";
import { createSdkworkAppbasePcAuthRuntime } from "@sdkwork/auth-runtime-pc-react";
import { createClient as createDeployClient, type SdkworkDeployAppClient } from "@sdkwork/deployments-app-sdk";
import { createClient as createIamClient } from "@sdkwork/iam-app-sdk";
import { createTokenManager, type AuthTokenManager } from "@sdkwork/sdk-common";

/**
 * Public, non-secret runtime document (`/runtime-env.json`). Same shape the PC
 * app checks in for the standalone development profile; per-profile documents
 * are materialized by the release tooling for other lanes.
 */
export interface DeploymentsH5RuntimeConfig {
  readonly environment: string;
  readonly deploymentProfile: string;
  readonly defaultLocale: string;
  readonly fallbackLocale: string;
  readonly appApiBaseUrl: string;
  readonly appbaseAppApiBaseUrl: string;
}

export interface DeploymentsH5Runtime {
  readonly config: DeploymentsH5RuntimeConfig;
  readonly locale: string;
  readonly tokenManager: AuthTokenManager;
  readonly deploy: SdkworkDeployAppClient;
  readonly authController: ReturnType<typeof createSdkworkIamRuntimeAuthController>;
}

declare global {
  // eslint-disable-next-line no-var
  var __SDKWORK_RUNTIME_ENV__: DeploymentsH5RuntimeConfig | undefined;
}

export async function loadDeploymentsH5RuntimeConfig(url = "/runtime-env.json"): Promise<DeploymentsH5RuntimeConfig> {
  const response = await fetch(url, { cache: "no-store" });
  if (!response.ok) {
    throw new Error("sdkwork-deployments-h5: public runtime config request failed with " + String(response.status));
  }
  const env = (await response.json()) as DeploymentsH5RuntimeConfig;
  globalThis.__SDKWORK_RUNTIME_ENV__ = env;
  return env;
}

function resolveAuthEnvironment(environment: string): "dev" | "test" | "prod" {
  return environment === "development" ? "dev" : environment === "test" ? "test" : "prod";
}

export async function bootstrapDeploymentsH5Runtime(): Promise<DeploymentsH5Runtime> {
  const config = await loadDeploymentsH5RuntimeConfig();
  const locale = config.defaultLocale === "en-US" ? "en-US" : "zh-CN";
  const tokenManager = createTokenManager();
  // The generated app client is constructed exactly once, here in bootstrap —
  // views and services receive it injected (`APP_SDK_INTEGRATION_SPEC`).
  const deploy = createDeployClient({
    baseUrl: config.appApiBaseUrl,
    authMode: "dual-token",
    platform: "h5",
    tokenManager,
  });
  const auth = createSdkworkAppbasePcAuthRuntime({
    app: {
      appId: "sdkwork-deployments-h5",
      deploymentMode: "saas",
      environment: resolveAuthEnvironment(config.environment),
      platform: "h5",
    },
    baseUrls: { appbaseAppApiBaseUrl: config.appbaseAppApiBaseUrl },
    createAppbaseAppClient: (clientConfig) =>
      createIamClient({
        ...clientConfig,
        timeout: config.environment === "production" || config.environment === "staging" ? 10_000 : 5_000,
      }),
    localeProvider: () => locale,
    sdkClients: [deploy],
    sessionAuth: true,
    tokenManager,
  });
  const getAuthRuntime = () => auth.getRuntime() as unknown as SdkworkIamRuntimeAuthRuntimeLike;
  return {
    config,
    locale,
    tokenManager,
    deploy,
    authController: createSdkworkIamRuntimeAuthController({ getRuntime: getAuthRuntime }),
  } as const;
}

export type BootstrappedDeploymentsH5Runtime = Awaited<ReturnType<typeof bootstrapDeploymentsH5Runtime>>;
