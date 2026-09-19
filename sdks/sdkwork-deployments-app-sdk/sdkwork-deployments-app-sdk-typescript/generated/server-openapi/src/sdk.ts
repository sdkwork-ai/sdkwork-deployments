import { HttpClient, createHttpClient } from './http/client';
import type { SdkworkAppConfig } from './types/common';
import type { AuthTokenManager } from '@sdkwork/sdk-common';

import { DomainApi, createDomainApi } from './api/domain';
import { CertificateApi, createCertificateApi } from './api/certificate';
import { UploadSessionApi, createUploadSessionApi } from './api/upload-session';
import { ArtifactApi, createArtifactApi } from './api/artifact';
import { AppApi, createAppApi } from './api/app';
import { EnvVariableApi, createEnvVariableApi } from './api/env-variable';
import { MonitorApi, createMonitorApi } from './api/monitor';
import { BuildApi, createBuildApi } from './api/build';
import { PackageApi, createPackageApi } from './api/package';
import { ReleaseApi, createReleaseApi } from './api/release';
import { DeploymentApi, createDeploymentApi } from './api/deployment';
import { SigningApi, createSigningApi } from './api/signing';
import { UsageApi, createUsageApi } from './api/usage';
import { AppDatabaseApi, createAppDatabaseApi } from './api/app-database';
import { AppEnvironmentApi, createAppEnvironmentApi } from './api/app-environment';

export class SdkworkDeployAppClient {
  private httpClient: HttpClient;

  public readonly domain: DomainApi;
  public readonly certificate: CertificateApi;
  public readonly uploadSession: UploadSessionApi;
  public readonly artifact: ArtifactApi;
  public readonly app: AppApi;
  public readonly envVariable: EnvVariableApi;
  public readonly monitor: MonitorApi;
  public readonly build: BuildApi;
  public readonly package: PackageApi;
  public readonly release: ReleaseApi;
  public readonly deployment: DeploymentApi;
  public readonly signing: SigningApi;
  public readonly usage: UsageApi;
  public readonly appDatabase: AppDatabaseApi;
  public readonly appEnvironment: AppEnvironmentApi;

  constructor(config: SdkworkAppConfig) {
    this.httpClient = createHttpClient(config);
    this.domain = createDomainApi(this.httpClient);

    this.certificate = createCertificateApi(this.httpClient);

    this.uploadSession = createUploadSessionApi(this.httpClient);

    this.artifact = createArtifactApi(this.httpClient);

    this.app = createAppApi(this.httpClient);

    this.envVariable = createEnvVariableApi(this.httpClient);

    this.monitor = createMonitorApi(this.httpClient);

    this.build = createBuildApi(this.httpClient);

    this.package = createPackageApi(this.httpClient);

    this.release = createReleaseApi(this.httpClient);

    this.deployment = createDeploymentApi(this.httpClient);

    this.signing = createSigningApi(this.httpClient);

    this.usage = createUsageApi(this.httpClient);

    this.appDatabase = createAppDatabaseApi(this.httpClient);

    this.appEnvironment = createAppEnvironmentApi(this.httpClient);
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

export function createClient(config: SdkworkAppConfig): SdkworkDeployAppClient {
  return new SdkworkDeployAppClient(config);
}

export default SdkworkDeployAppClient;
