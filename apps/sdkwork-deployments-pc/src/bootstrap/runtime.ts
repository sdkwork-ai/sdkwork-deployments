import { createSdkworkIamRuntimeAuthController, type SdkworkIamRuntimeAuthRuntimeLike } from "@sdkwork/auth-pc-react";
import { createSdkworkAppbasePcAuthRuntime } from "@sdkwork/auth-runtime-pc-react";
import { createDeploymentsConsoleClients } from "@sdkwork/deployments-pc-console-core";
import { loadDeploymentsRuntimeConfig, resolveDeploymentsLocale } from "@sdkwork/deployments-pc-core";
import { createClient as createIamClient } from "@sdkwork/iam-app-sdk";
import { createTokenManager } from "@sdkwork/sdk-common";

export async function bootstrapDeploymentsRuntime() {
  const config = await loadDeploymentsRuntimeConfig();
  const locale = resolveDeploymentsLocale(config, navigator.languages);
  const tokenManager = createTokenManager();
  const clients = createDeploymentsConsoleClients({ deployBaseUrl: config.appApiBaseUrl, driveBaseUrl: config.driveAppApiBaseUrl, tokenManager });
  const auth = createSdkworkAppbasePcAuthRuntime({
    app: { appId: "sdkwork-deployments-pc", deploymentMode: "saas", environment: config.environment === "development" ? "dev" : config.environment === "test" ? "test" : "prod", platform: "pc" },
    baseUrls: { appbaseAppApiBaseUrl: config.appbaseAppApiBaseUrl },
    createAppbaseAppClient: (clientConfig) => createIamClient({ ...clientConfig, timeout: config.environment === "production" || config.environment === "staging" ? 10_000 : 5_000 }),
    localeProvider: () => locale,
    sdkClients: [clients.deploy, clients.drive, clients.order],
    sessionAuth: true,
    tokenManager,
  });
  const getAuthRuntime = () => auth.getRuntime() as unknown as SdkworkIamRuntimeAuthRuntimeLike;
  return { auth, authController: createSdkworkIamRuntimeAuthController({ getRuntime: getAuthRuntime }), clients, config, locale, tokenManager } as const;
}

export type BootstrappedDeploymentsRuntime = Awaited<ReturnType<typeof bootstrapDeploymentsRuntime>>;
