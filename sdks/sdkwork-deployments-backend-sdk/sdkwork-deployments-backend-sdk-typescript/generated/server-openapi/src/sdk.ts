import { HttpClient, createHttpClient } from './http/client';
import type { SdkworkBackendConfig } from './types/common';
import type { AuthTokenManager } from '@sdkwork/sdk-common';

import { NginxApi, createNginxApi } from './api/nginx';
import { ServerApi, createServerApi } from './api/server';
import { ClusterApi, createClusterApi } from './api/cluster';
import { AuditApi, createAuditApi } from './api/audit';
import { EntitlementApi, createEntitlementApi } from './api/entitlement';
import { BuildQueueApi, createBuildQueueApi } from './api/build-queue';
import { RunnersApi, createRunnersApi } from './api/runners';
import { TlsApi, createTlsApi } from './api/tls';
import { RetentionApi, createRetentionApi } from './api/retention';
import { UsageApi, createUsageApi } from './api/usage';
import { SigningHealthApi, createSigningHealthApi } from './api/signing-health';
import { SourceEventsApi, createSourceEventsApi } from './api/source-events';
import { TemplateApi, createTemplateApi } from './api/template';

export class SdkworkDeployBackendClient {
  private httpClient: HttpClient;

  public readonly nginx: NginxApi;
  public readonly server: ServerApi;
  public readonly cluster: ClusterApi;
  public readonly audit: AuditApi;
  public readonly entitlement: EntitlementApi;
  public readonly buildQueue: BuildQueueApi;
  public readonly runners: RunnersApi;
  public readonly tls: TlsApi;
  public readonly retention: RetentionApi;
  public readonly usage: UsageApi;
  public readonly signingHealth: SigningHealthApi;
  public readonly sourceEvents: SourceEventsApi;
  public readonly template: TemplateApi;

  constructor(config: SdkworkBackendConfig) {
    this.httpClient = createHttpClient(config);
    this.nginx = createNginxApi(this.httpClient);

    this.server = createServerApi(this.httpClient);

    this.cluster = createClusterApi(this.httpClient);

    this.audit = createAuditApi(this.httpClient);

    this.entitlement = createEntitlementApi(this.httpClient);

    this.buildQueue = createBuildQueueApi(this.httpClient);

    this.runners = createRunnersApi(this.httpClient);

    this.tls = createTlsApi(this.httpClient);

    this.retention = createRetentionApi(this.httpClient);

    this.usage = createUsageApi(this.httpClient);

    this.signingHealth = createSigningHealthApi(this.httpClient);

    this.sourceEvents = createSourceEventsApi(this.httpClient);

    this.template = createTemplateApi(this.httpClient);
  }
  setAuthToken(token: string): this {
    this.httpClient.setAuthToken(token);
    return this;
  }

  setAccessToken(token: string): this {
    this.httpClient.setAccessToken(token);
    return this;
  }

  setTokenManager(manager: AuthTokenManager): this {
    this.httpClient.setTokenManager(manager);
    return this;
  }

  get http(): HttpClient {
    return this.httpClient;
  }
}

export function createClient(config: SdkworkBackendConfig): SdkworkDeployBackendClient {
  return new SdkworkDeployBackendClient(config);
}

export default SdkworkDeployBackendClient;
