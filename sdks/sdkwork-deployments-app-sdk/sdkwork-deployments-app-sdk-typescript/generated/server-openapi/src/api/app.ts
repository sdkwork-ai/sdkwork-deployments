import { appApiPath } from './paths';
import type { ApiRequestOptions, HttpClient } from '../http/client';

import type { AppCompositionResponse, AppDomainResponse, AppFactoryReadiness, AppKind, ApplicationScaffoldInstallResponse, ApplicationScaffoldResponse, ApplicationScaffoldSummaryResponse, ApplicationScaffoldVersionResponse, AppOwnerType, AppPublishEnvironment, AppRemixResponse, AppResourceKind, AppResourceLinkResponse, AppResponse, AppSourceResponse, AppSourceSpecResponse, AppUninstallResponse, BindAppSourceSpecSourceRequest, CreateApplicationScaffoldRequest, CreateApplicationScaffoldVersionRequest, CreateAppRequest, CreateAppSourceSpecRequest, CreatePlatformTargetRequest, CreateSourceRepositoryRequest, InstallApplicationScaffoldRequest, PageInfo, PlatformTargetResponse, ScaffoldCategoryResponse, SourceRepositoryResponse, UpdateAppCompositionRequest, UpdateApplicationScaffoldRequest, UpdateAppRemixRequest, UpdateAppRequest, UpdateAppSourceRequest, UpdateAppSourceSpecRequest, UpsertAppResourceLinkRequest } from '../types';


export interface AppMarketplaceScaffoldsListParams {
  page?: number;
  pageSize?: number;
  keyword?: string;
  categoryUuid?: string;
  pricingModel?: 'FREE' | 'PAID';
  templateType?: 'APP' | 'PPT' | 'VIDEO';
  sort?: 'NEWEST' | 'POPULAR';
}

export interface AppMarketplaceScaffoldsInstallParams {
  idempotencyKey: string;
}

export class AppMarketplaceScaffoldsApi {
  private client: HttpClient;

  constructor(client: HttpClient) {
    this.client = client;
  }


/** Browse the tenant application scaffold marketplace */
  async list(params?: AppMarketplaceScaffoldsListParams, requestOptions?: ApiRequestOptions): Promise<{ items: ApplicationScaffoldSummaryResponse[]; pageInfo: PageInfo; }> {
    const query = buildQueryString([
      { name: 'page', value: params?.page, style: 'form', explode: true, allowReserved: false },
      { name: 'page_size', value: params?.pageSize, style: 'form', explode: true, allowReserved: false },
      { name: 'keyword', value: params?.keyword, style: 'form', explode: true, allowReserved: false },
      { name: 'category_uuid', value: params?.categoryUuid, style: 'form', explode: true, allowReserved: false },
      { name: 'pricing_model', value: params?.pricingModel, style: 'form', explode: true, allowReserved: false },
      { name: 'template_type', value: params?.templateType, style: 'form', explode: true, allowReserved: false },
      { name: 'sort', value: params?.sort, style: 'form', explode: true, allowReserved: false },
    ]);
    return this.client.request<{ items: ApplicationScaffoldSummaryResponse[]; pageInfo: PageInfo; }>(appendQueryString(appApiPath(`/marketplace/scaffolds`), query), { ...(requestOptions?.signal !== undefined ? { signal: requestOptions.signal } : {}), ...(requestOptions?.timeout !== undefined ? { timeout: requestOptions.timeout } : {}), method: 'GET' as any, sdkworkUnwrapKind: 'page' });
  }

/** Retrieve a marketplace application scaffold */
  async retrieve(scaffoldUuid: string, requestOptions?: ApiRequestOptions): Promise<ApplicationScaffoldResponse> {
    return this.client.request<ApplicationScaffoldResponse>(appApiPath(`/marketplace/scaffolds/${serializePathParameter(scaffoldUuid, { name: 'scaffoldUuid', style: 'simple', explode: false })}`), { ...(requestOptions?.signal !== undefined ? { signal: requestOptions.signal } : {}), ...(requestOptions?.timeout !== undefined ? { timeout: requestOptions.timeout } : {}), method: 'GET' as any, sdkworkUnwrapKind: 'item' });
  }

/** Install a free, Git-backed template as a new application */
  async install(scaffoldUuid: string, body: InstallApplicationScaffoldRequest, params: AppMarketplaceScaffoldsInstallParams, requestOptions?: ApiRequestOptions): Promise<ApplicationScaffoldInstallResponse> {
    const requestHeaders = buildRequestHeaders(
      {
        'Idempotency-Key': { value: params.idempotencyKey, style: 'simple', explode: false },
      },
      {}
    );
    return this.client.request<ApplicationScaffoldInstallResponse>(appApiPath(`/marketplace/scaffolds/${serializePathParameter(scaffoldUuid, { name: 'scaffoldUuid', style: 'simple', explode: false })}/install`), { ...(requestOptions?.signal !== undefined ? { signal: requestOptions.signal } : {}), ...(requestOptions?.timeout !== undefined ? { timeout: requestOptions.timeout } : {}), method: 'POST' as any, body, contentType: 'application/json', ...(requestHeaders !== undefined ? { headers: requestHeaders } : {}), sdkworkUnwrapKind: 'item' });
  }
}

export class AppMarketplaceApi {
  public readonly scaffolds: AppMarketplaceScaffoldsApi;

  constructor(client: HttpClient) {
    this.scaffolds = new AppMarketplaceScaffoldsApi(client);
  }

}

export interface AppScaffoldsVersionsListParams {
  page?: number;
  pageSize?: number;
}

export interface AppScaffoldsVersionsCreateParams {
  idempotencyKey: string;
}

export class AppScaffoldsVersionsApi {
  private client: HttpClient;

  constructor(client: HttpClient) {
    this.client = client;
  }


/** List versions of the caller's application scaffold */
  async list(scaffoldUuid: string, params?: AppScaffoldsVersionsListParams, requestOptions?: ApiRequestOptions): Promise<{ items: ApplicationScaffoldVersionResponse[]; pageInfo: PageInfo; }> {
    const query = buildQueryString([
      { name: 'page', value: params?.page, style: 'form', explode: true, allowReserved: false },
      { name: 'page_size', value: params?.pageSize, style: 'form', explode: true, allowReserved: false },
    ]);
    return this.client.request<{ items: ApplicationScaffoldVersionResponse[]; pageInfo: PageInfo; }>(appendQueryString(appApiPath(`/scaffolds/${serializePathParameter(scaffoldUuid, { name: 'scaffoldUuid', style: 'simple', explode: false })}/versions`), query), { ...(requestOptions?.signal !== undefined ? { signal: requestOptions.signal } : {}), ...(requestOptions?.timeout !== undefined ? { timeout: requestOptions.timeout } : {}), method: 'GET' as any, sdkworkUnwrapKind: 'page' });
  }

/** Add a version to the caller's application scaffold */
  async create(scaffoldUuid: string, body: CreateApplicationScaffoldVersionRequest, params: AppScaffoldsVersionsCreateParams, requestOptions?: ApiRequestOptions): Promise<ApplicationScaffoldVersionResponse> {
    const requestHeaders = buildRequestHeaders(
      {
        'Idempotency-Key': { value: params.idempotencyKey, style: 'simple', explode: false },
      },
      {}
    );
    return this.client.request<ApplicationScaffoldVersionResponse>(appApiPath(`/scaffolds/${serializePathParameter(scaffoldUuid, { name: 'scaffoldUuid', style: 'simple', explode: false })}/versions`), { ...(requestOptions?.signal !== undefined ? { signal: requestOptions.signal } : {}), ...(requestOptions?.timeout !== undefined ? { timeout: requestOptions.timeout } : {}), method: 'POST' as any, body, contentType: 'application/json', ...(requestHeaders !== undefined ? { headers: requestHeaders } : {}), sdkworkUnwrapKind: 'item' });
  }
}

export interface AppScaffoldsCategoriesListParams {
  includeDisabled?: boolean;
}

export class AppScaffoldsCategoriesApi {
  private client: HttpClient;

  constructor(client: HttpClient) {
    this.client = client;
  }


/** List template categories */
  async list(params?: AppScaffoldsCategoriesListParams, requestOptions?: ApiRequestOptions): Promise<{ items: ScaffoldCategoryResponse[]; pageInfo: PageInfo; }> {
    const query = buildQueryString([
      { name: 'include_disabled', value: params?.includeDisabled, style: 'form', explode: true, allowReserved: false },
    ]);
    return this.client.request<{ items: ScaffoldCategoryResponse[]; pageInfo: PageInfo; }>(appendQueryString(appApiPath(`/scaffold_categories`), query), { ...(requestOptions?.signal !== undefined ? { signal: requestOptions.signal } : {}), ...(requestOptions?.timeout !== undefined ? { timeout: requestOptions.timeout } : {}), method: 'GET' as any, sdkworkUnwrapKind: 'page' });
  }
}

export interface AppScaffoldsListParams {
  page?: number;
  pageSize?: number;
  keyword?: string;
  status?: 'DRAFT' | 'PENDING_REVIEW' | 'PUBLISHED' | 'REJECTED' | 'DISABLED';
}

export interface AppScaffoldsCreateParams {
  idempotencyKey: string;
}

export interface AppScaffoldsUpdateParams {
  idempotencyKey: string;
}

export class AppScaffoldsApi {
  private client: HttpClient;
  public readonly categories: AppScaffoldsCategoriesApi;
  public readonly versions: AppScaffoldsVersionsApi;

  constructor(client: HttpClient) {
    this.client = client;
    this.categories = new AppScaffoldsCategoriesApi(client);
    this.versions = new AppScaffoldsVersionsApi(client);
  }


/** List the caller's application scaffolds */
  async list(params?: AppScaffoldsListParams, requestOptions?: ApiRequestOptions): Promise<{ items: ApplicationScaffoldResponse[]; pageInfo: PageInfo; }> {
    const query = buildQueryString([
      { name: 'page', value: params?.page, style: 'form', explode: true, allowReserved: false },
      { name: 'page_size', value: params?.pageSize, style: 'form', explode: true, allowReserved: false },
      { name: 'keyword', value: params?.keyword, style: 'form', explode: true, allowReserved: false },
      { name: 'status', value: params?.status, style: 'form', explode: true, allowReserved: false },
    ]);
    return this.client.request<{ items: ApplicationScaffoldResponse[]; pageInfo: PageInfo; }>(appendQueryString(appApiPath(`/scaffolds`), query), { ...(requestOptions?.signal !== undefined ? { signal: requestOptions.signal } : {}), ...(requestOptions?.timeout !== undefined ? { timeout: requestOptions.timeout } : {}), method: 'GET' as any, sdkworkUnwrapKind: 'page' });
  }

/** Publish an application as a scaffold listing */
  async create(body: CreateApplicationScaffoldRequest, params: AppScaffoldsCreateParams, requestOptions?: ApiRequestOptions): Promise<ApplicationScaffoldResponse> {
    const requestHeaders = buildRequestHeaders(
      {
        'Idempotency-Key': { value: params.idempotencyKey, style: 'simple', explode: false },
      },
      {}
    );
    return this.client.request<ApplicationScaffoldResponse>(appApiPath(`/scaffolds`), { ...(requestOptions?.signal !== undefined ? { signal: requestOptions.signal } : {}), ...(requestOptions?.timeout !== undefined ? { timeout: requestOptions.timeout } : {}), method: 'POST' as any, body, contentType: 'application/json', ...(requestHeaders !== undefined ? { headers: requestHeaders } : {}), sdkworkUnwrapKind: 'item' });
  }

/** Retrieve the caller's application scaffold */
  async retrieve(scaffoldUuid: string, requestOptions?: ApiRequestOptions): Promise<ApplicationScaffoldResponse> {
    return this.client.request<ApplicationScaffoldResponse>(appApiPath(`/scaffolds/${serializePathParameter(scaffoldUuid, { name: 'scaffoldUuid', style: 'simple', explode: false })}`), { ...(requestOptions?.signal !== undefined ? { signal: requestOptions.signal } : {}), ...(requestOptions?.timeout !== undefined ? { timeout: requestOptions.timeout } : {}), method: 'GET' as any, sdkworkUnwrapKind: 'item' });
  }

/** Update the caller's application scaffold listing */
  async update(scaffoldUuid: string, body: UpdateApplicationScaffoldRequest, params: AppScaffoldsUpdateParams, requestOptions?: ApiRequestOptions): Promise<ApplicationScaffoldResponse> {
    const requestHeaders = buildRequestHeaders(
      {
        'Idempotency-Key': { value: params.idempotencyKey, style: 'simple', explode: false },
      },
      {}
    );
    return this.client.request<ApplicationScaffoldResponse>(appApiPath(`/scaffolds/${serializePathParameter(scaffoldUuid, { name: 'scaffoldUuid', style: 'simple', explode: false })}`), { ...(requestOptions?.signal !== undefined ? { signal: requestOptions.signal } : {}), ...(requestOptions?.timeout !== undefined ? { timeout: requestOptions.timeout } : {}), method: 'PATCH' as any, body, contentType: 'application/json', ...(requestHeaders !== undefined ? { headers: requestHeaders } : {}), sdkworkUnwrapKind: 'item' });
  }

/** Withdraw the caller's application scaffold */
  async delete(scaffoldUuid: string, requestOptions?: ApiRequestOptions): Promise<void> {
    return this.client.request<void>(appApiPath(`/scaffolds/${serializePathParameter(scaffoldUuid, { name: 'scaffoldUuid', style: 'simple', explode: false })}`), { ...(requestOptions?.signal !== undefined ? { signal: requestOptions.signal } : {}), ...(requestOptions?.timeout !== undefined ? { timeout: requestOptions.timeout } : {}), method: 'DELETE' as any });
  }

/** Submit an application scaffold for review */
  async submit(scaffoldUuid: string, requestOptions?: ApiRequestOptions): Promise<ApplicationScaffoldResponse> {
    return this.client.request<ApplicationScaffoldResponse>(appApiPath(`/scaffolds/${serializePathParameter(scaffoldUuid, { name: 'scaffoldUuid', style: 'simple', explode: false })}/submit`), { ...(requestOptions?.signal !== undefined ? { signal: requestOptions.signal } : {}), ...(requestOptions?.timeout !== undefined ? { timeout: requestOptions.timeout } : {}), method: 'POST' as any, sdkworkUnwrapKind: 'item' });
  }
}

export interface AppSourceRepositoriesCreateParams {
  idempotencyKey: string;
}

export class AppSourceRepositoriesApi {
  private client: HttpClient;

  constructor(client: HttpClient) {
    this.client = client;
  }


/** List source repositories of an app */
  async list(appId: string, requestOptions?: ApiRequestOptions): Promise<{ items: SourceRepositoryResponse[]; pageInfo: PageInfo; }> {
    return this.client.request<{ items: SourceRepositoryResponse[]; pageInfo: PageInfo; }>(appApiPath(`/apps/${serializePathParameter(appId, { name: 'appId', style: 'simple', explode: false })}/source_repositories`), { ...(requestOptions?.signal !== undefined ? { signal: requestOptions.signal } : {}), ...(requestOptions?.timeout !== undefined ? { timeout: requestOptions.timeout } : {}), method: 'GET' as any, sdkworkUnwrapKind: 'page' });
  }

/** Bind a Git source repository to an app */
  async create(appId: string, body: CreateSourceRepositoryRequest, params: AppSourceRepositoriesCreateParams, requestOptions?: ApiRequestOptions): Promise<SourceRepositoryResponse> {
    const requestHeaders = buildRequestHeaders(
      {
        'Idempotency-Key': { value: params.idempotencyKey, style: 'simple', explode: false },
      },
      {}
    );
    return this.client.request<SourceRepositoryResponse>(appApiPath(`/apps/${serializePathParameter(appId, { name: 'appId', style: 'simple', explode: false })}/source_repositories`), { ...(requestOptions?.signal !== undefined ? { signal: requestOptions.signal } : {}), ...(requestOptions?.timeout !== undefined ? { timeout: requestOptions.timeout } : {}), method: 'POST' as any, body, contentType: 'application/json', ...(requestHeaders !== undefined ? { headers: requestHeaders } : {}), sdkworkUnwrapKind: 'item' });
  }

/** Retrieve a source repository */
  async retrieve(appId: string, sourceRepositoryId: string, requestOptions?: ApiRequestOptions): Promise<SourceRepositoryResponse> {
    return this.client.request<SourceRepositoryResponse>(appApiPath(`/apps/${serializePathParameter(appId, { name: 'appId', style: 'simple', explode: false })}/source_repositories/${serializePathParameter(sourceRepositoryId, { name: 'sourceRepositoryId', style: 'simple', explode: false })}`), { ...(requestOptions?.signal !== undefined ? { signal: requestOptions.signal } : {}), ...(requestOptions?.timeout !== undefined ? { timeout: requestOptions.timeout } : {}), method: 'GET' as any, sdkworkUnwrapKind: 'item' });
  }
}

export interface AppPlatformTargetsCreateParams {
  idempotencyKey: string;
}

export class AppPlatformTargetsApi {
  private client: HttpClient;

  constructor(client: HttpClient) {
    this.client = client;
  }


/** List platform targets of an app */
  async list(appId: string, requestOptions?: ApiRequestOptions): Promise<{ items: PlatformTargetResponse[]; pageInfo: PageInfo; }> {
    return this.client.request<{ items: PlatformTargetResponse[]; pageInfo: PageInfo; }>(appApiPath(`/apps/${serializePathParameter(appId, { name: 'appId', style: 'simple', explode: false })}/platform_targets`), { ...(requestOptions?.signal !== undefined ? { signal: requestOptions.signal } : {}), ...(requestOptions?.timeout !== undefined ? { timeout: requestOptions.timeout } : {}), method: 'GET' as any, sdkworkUnwrapKind: 'page' });
  }

/** Create a platform target */
  async create(appId: string, body: CreatePlatformTargetRequest, params: AppPlatformTargetsCreateParams, requestOptions?: ApiRequestOptions): Promise<PlatformTargetResponse> {
    const requestHeaders = buildRequestHeaders(
      {
        'Idempotency-Key': { value: params.idempotencyKey, style: 'simple', explode: false },
      },
      {}
    );
    return this.client.request<PlatformTargetResponse>(appApiPath(`/apps/${serializePathParameter(appId, { name: 'appId', style: 'simple', explode: false })}/platform_targets`), { ...(requestOptions?.signal !== undefined ? { signal: requestOptions.signal } : {}), ...(requestOptions?.timeout !== undefined ? { timeout: requestOptions.timeout } : {}), method: 'POST' as any, body, contentType: 'application/json', ...(requestHeaders !== undefined ? { headers: requestHeaders } : {}), sdkworkUnwrapKind: 'item' });
  }

/** Retrieve a platform target */
  async retrieve(appId: string, platformTargetId: string, requestOptions?: ApiRequestOptions): Promise<PlatformTargetResponse> {
    return this.client.request<PlatformTargetResponse>(appApiPath(`/apps/${serializePathParameter(appId, { name: 'appId', style: 'simple', explode: false })}/platform_targets/${serializePathParameter(platformTargetId, { name: 'platformTargetId', style: 'simple', explode: false })}`), { ...(requestOptions?.signal !== undefined ? { signal: requestOptions.signal } : {}), ...(requestOptions?.timeout !== undefined ? { timeout: requestOptions.timeout } : {}), method: 'GET' as any, sdkworkUnwrapKind: 'item' });
  }
}

export interface AppResourceLinksListParams {
  environment?: AppPublishEnvironment;
  resourceKind?: AppResourceKind;
  page?: number;
  pageSize?: number;
}

export class AppResourceLinksApi {
  private client: HttpClient;

  constructor(client: HttpClient) {
    this.client = client;
  }


/** List an application's account bindings */
  async list(appId: string, params?: AppResourceLinksListParams, requestOptions?: ApiRequestOptions): Promise<{ items: AppResourceLinkResponse[]; pageInfo: PageInfo; }> {
    const query = buildQueryString([
      { name: 'environment', value: params?.environment, style: 'form', explode: true, allowReserved: false },
      { name: 'resource_kind', value: params?.resourceKind, style: 'form', explode: true, allowReserved: false },
      { name: 'page', value: params?.page, style: 'form', explode: true, allowReserved: false },
      { name: 'page_size', value: params?.pageSize, style: 'form', explode: true, allowReserved: false },
    ]);
    return this.client.request<{ items: AppResourceLinkResponse[]; pageInfo: PageInfo; }>(appendQueryString(appApiPath(`/apps/${serializePathParameter(appId, { name: 'appId', style: 'simple', explode: false })}/resource_links`), query), { ...(requestOptions?.signal !== undefined ? { signal: requestOptions.signal } : {}), ...(requestOptions?.timeout !== undefined ? { timeout: requestOptions.timeout } : {}), method: 'GET' as any, sdkworkUnwrapKind: 'page' });
  }

/** Bind or replace an application's account binding */
  async update(appId: string, body: UpsertAppResourceLinkRequest, requestOptions?: ApiRequestOptions): Promise<AppResourceLinkResponse> {
    return this.client.request<AppResourceLinkResponse>(appApiPath(`/apps/${serializePathParameter(appId, { name: 'appId', style: 'simple', explode: false })}/resource_links`), { ...(requestOptions?.signal !== undefined ? { signal: requestOptions.signal } : {}), ...(requestOptions?.timeout !== undefined ? { timeout: requestOptions.timeout } : {}), method: 'PUT' as any, body, contentType: 'application/json', sdkworkUnwrapKind: 'item' });
  }

/** Retire an account binding */
  async delete(appId: string, resourceLinkId: string, requestOptions?: ApiRequestOptions): Promise<void> {
    return this.client.request<void>(appApiPath(`/apps/${serializePathParameter(appId, { name: 'appId', style: 'simple', explode: false })}/resource_links/${serializePathParameter(resourceLinkId, { name: 'resourceLinkId', style: 'simple', explode: false })}`), { ...(requestOptions?.signal !== undefined ? { signal: requestOptions.signal } : {}), ...(requestOptions?.timeout !== undefined ? { timeout: requestOptions.timeout } : {}), method: 'DELETE' as any });
  }
}

export class AppRemixApi {
  private client: HttpClient;

  constructor(client: HttpClient) {
    this.client = client;
  }


/** Retrieve the remix state and download instructions */
  async retrieve(appId: string, requestOptions?: ApiRequestOptions): Promise<AppRemixResponse> {
    return this.client.request<AppRemixResponse>(appApiPath(`/apps/${serializePathParameter(appId, { name: 'appId', style: 'simple', explode: false })}/remix`), { ...(requestOptions?.signal !== undefined ? { signal: requestOptions.signal } : {}), ...(requestOptions?.timeout !== undefined ? { timeout: requestOptions.timeout } : {}), method: 'GET' as any, sdkworkUnwrapKind: 'item' });
  }

/** Enable or disable remix */
  async update(appId: string, body: UpdateAppRemixRequest, requestOptions?: ApiRequestOptions): Promise<AppRemixResponse> {
    return this.client.request<AppRemixResponse>(appApiPath(`/apps/${serializePathParameter(appId, { name: 'appId', style: 'simple', explode: false })}/remix`), { ...(requestOptions?.signal !== undefined ? { signal: requestOptions.signal } : {}), ...(requestOptions?.timeout !== undefined ? { timeout: requestOptions.timeout } : {}), method: 'PUT' as any, body, contentType: 'application/json', sdkworkUnwrapKind: 'item' });
  }
}

export class AppSourceApi {
  private client: HttpClient;

  constructor(client: HttpClient) {
    this.client = client;
  }


/** Retrieve the application's remix source */
  async retrieve(appId: string, requestOptions?: ApiRequestOptions): Promise<AppSourceResponse> {
    return this.client.request<AppSourceResponse>(appApiPath(`/apps/${serializePathParameter(appId, { name: 'appId', style: 'simple', explode: false })}/source`), { ...(requestOptions?.signal !== undefined ? { signal: requestOptions.signal } : {}), ...(requestOptions?.timeout !== undefined ? { timeout: requestOptions.timeout } : {}), method: 'GET' as any, sdkworkUnwrapKind: 'item' });
  }

/** Replace the application's remix source */
  async update(appId: string, body: UpdateAppSourceRequest, requestOptions?: ApiRequestOptions): Promise<AppSourceResponse> {
    return this.client.request<AppSourceResponse>(appApiPath(`/apps/${serializePathParameter(appId, { name: 'appId', style: 'simple', explode: false })}/source`), { ...(requestOptions?.signal !== undefined ? { signal: requestOptions.signal } : {}), ...(requestOptions?.timeout !== undefined ? { timeout: requestOptions.timeout } : {}), method: 'PUT' as any, body, contentType: 'application/json', sdkworkUnwrapKind: 'item' });
  }

/** Retire the application's remix source */
  async delete(appId: string, requestOptions?: ApiRequestOptions): Promise<void> {
    return this.client.request<void>(appApiPath(`/apps/${serializePathParameter(appId, { name: 'appId', style: 'simple', explode: false })}/source`), { ...(requestOptions?.signal !== undefined ? { signal: requestOptions.signal } : {}), ...(requestOptions?.timeout !== undefined ? { timeout: requestOptions.timeout } : {}), method: 'DELETE' as any });
  }

/** Validate the application's Git source repository */
  async validate(appId: string, requestOptions?: ApiRequestOptions): Promise<AppSourceResponse> {
    return this.client.request<AppSourceResponse>(appApiPath(`/apps/${serializePathParameter(appId, { name: 'appId', style: 'simple', explode: false })}/source/validate`), { ...(requestOptions?.signal !== undefined ? { signal: requestOptions.signal } : {}), ...(requestOptions?.timeout !== undefined ? { timeout: requestOptions.timeout } : {}), method: 'POST' as any, sdkworkUnwrapKind: 'item' });
  }
}

export class AppSourceSpecsSourceApi {
  private client: HttpClient;

  constructor(client: HttpClient) {
    this.client = client;
  }


/** Register the uploaded source behind a spec */
  async update(appId: string, specId: string, body: BindAppSourceSpecSourceRequest, requestOptions?: ApiRequestOptions): Promise<AppSourceSpecResponse> {
    return this.client.request<AppSourceSpecResponse>(appApiPath(`/apps/${serializePathParameter(appId, { name: 'appId', style: 'simple', explode: false })}/source_specs/${serializePathParameter(specId, { name: 'specId', style: 'simple', explode: false })}/source`), { ...(requestOptions?.signal !== undefined ? { signal: requestOptions.signal } : {}), ...(requestOptions?.timeout !== undefined ? { timeout: requestOptions.timeout } : {}), method: 'PUT' as any, body, contentType: 'application/json', sdkworkUnwrapKind: 'item' });
  }
}

export interface AppSourceSpecsListParams {
  environment?: AppPublishEnvironment;
}

export interface AppSourceSpecsCreateParams {
  idempotencyKey: string;
}

export interface AppSourceSpecsUpdateParams {
  ifMatch: string;
}

export class AppSourceSpecsApi {
  private client: HttpClient;
  public readonly source: AppSourceSpecsSourceApi;

  constructor(client: HttpClient) {
    this.client = client;
    this.source = new AppSourceSpecsSourceApi(client);
  }


/** List an app's source specs */
  async list(appId: string, params?: AppSourceSpecsListParams, requestOptions?: ApiRequestOptions): Promise<{ items: AppSourceSpecResponse[]; pageInfo: PageInfo; }> {
    const query = buildQueryString([
      { name: 'environment', value: params?.environment, style: 'form', explode: true, allowReserved: false },
    ]);
    return this.client.request<{ items: AppSourceSpecResponse[]; pageInfo: PageInfo; }>(appendQueryString(appApiPath(`/apps/${serializePathParameter(appId, { name: 'appId', style: 'simple', explode: false })}/source_specs`), query), { ...(requestOptions?.signal !== undefined ? { signal: requestOptions.signal } : {}), ...(requestOptions?.timeout !== undefined ? { timeout: requestOptions.timeout } : {}), method: 'GET' as any, sdkworkUnwrapKind: 'page' });
  }

/** Declare a source spec for an app */
  async create(appId: string, body: CreateAppSourceSpecRequest, params: AppSourceSpecsCreateParams, requestOptions?: ApiRequestOptions): Promise<AppSourceSpecResponse> {
    const requestHeaders = buildRequestHeaders(
      {
        'Idempotency-Key': { value: params.idempotencyKey, style: 'simple', explode: false },
      },
      {}
    );
    return this.client.request<AppSourceSpecResponse>(appApiPath(`/apps/${serializePathParameter(appId, { name: 'appId', style: 'simple', explode: false })}/source_specs`), { ...(requestOptions?.signal !== undefined ? { signal: requestOptions.signal } : {}), ...(requestOptions?.timeout !== undefined ? { timeout: requestOptions.timeout } : {}), method: 'POST' as any, body, contentType: 'application/json', ...(requestHeaders !== undefined ? { headers: requestHeaders } : {}), sdkworkUnwrapKind: 'item' });
  }

/** Retrieve a source spec */
  async retrieve(appId: string, specId: string, requestOptions?: ApiRequestOptions): Promise<AppSourceSpecResponse> {
    return this.client.request<AppSourceSpecResponse>(appApiPath(`/apps/${serializePathParameter(appId, { name: 'appId', style: 'simple', explode: false })}/source_specs/${serializePathParameter(specId, { name: 'specId', style: 'simple', explode: false })}`), { ...(requestOptions?.signal !== undefined ? { signal: requestOptions.signal } : {}), ...(requestOptions?.timeout !== undefined ? { timeout: requestOptions.timeout } : {}), method: 'GET' as any, sdkworkUnwrapKind: 'item' });
  }

/** Update a source spec */
  async update(appId: string, specId: string, body: UpdateAppSourceSpecRequest, params: AppSourceSpecsUpdateParams, requestOptions?: ApiRequestOptions): Promise<AppSourceSpecResponse> {
    const requestHeaders = buildRequestHeaders(
      {
        'If-Match': { value: params.ifMatch, style: 'simple', explode: false },
      },
      {}
    );
    return this.client.request<AppSourceSpecResponse>(appApiPath(`/apps/${serializePathParameter(appId, { name: 'appId', style: 'simple', explode: false })}/source_specs/${serializePathParameter(specId, { name: 'specId', style: 'simple', explode: false })}`), { ...(requestOptions?.signal !== undefined ? { signal: requestOptions.signal } : {}), ...(requestOptions?.timeout !== undefined ? { timeout: requestOptions.timeout } : {}), method: 'PATCH' as any, body, contentType: 'application/json', ...(requestHeaders !== undefined ? { headers: requestHeaders } : {}), sdkworkUnwrapKind: 'item' });
  }

/** Delete a source spec */
  async delete(appId: string, specId: string, requestOptions?: ApiRequestOptions): Promise<void> {
    return this.client.request<void>(appApiPath(`/apps/${serializePathParameter(appId, { name: 'appId', style: 'simple', explode: false })}/source_specs/${serializePathParameter(specId, { name: 'specId', style: 'simple', explode: false })}`), { ...(requestOptions?.signal !== undefined ? { signal: requestOptions.signal } : {}), ...(requestOptions?.timeout !== undefined ? { timeout: requestOptions.timeout } : {}), method: 'DELETE' as any });
  }
}

export interface AppCompositionUpdateParams {
  ifMatch: string;
  idempotencyKey: string;
}

export class AppCompositionApi {
  private client: HttpClient;

  constructor(client: HttpClient) {
    this.client = client;
  }


/** Replace and publish the complete live app composition */
  async update(appId: string, body: UpdateAppCompositionRequest, params: AppCompositionUpdateParams, requestOptions?: ApiRequestOptions): Promise<AppCompositionResponse> {
    const requestHeaders = buildRequestHeaders(
      {
        'If-Match': { value: params.ifMatch, style: 'simple', explode: false },
        'Idempotency-Key': { value: params.idempotencyKey, style: 'simple', explode: false },
      },
      {}
    );
    return this.client.request<AppCompositionResponse>(appApiPath(`/apps/${serializePathParameter(appId, { name: 'appId', style: 'simple', explode: false })}/composition`), { ...(requestOptions?.signal !== undefined ? { signal: requestOptions.signal } : {}), ...(requestOptions?.timeout !== undefined ? { timeout: requestOptions.timeout } : {}), method: 'PUT' as any, body, contentType: 'application/json', ...(requestHeaders !== undefined ? { headers: requestHeaders } : {}), sdkworkUnwrapKind: 'item' });
  }
}

export class AppDomainsApi {
  private client: HttpClient;

  constructor(client: HttpClient) {
    this.client = client;
  }


/** List an app's publishing domains */
  async list(appId: string, requestOptions?: ApiRequestOptions): Promise<{ items: AppDomainResponse[]; }> {
    return this.client.request<{ items: AppDomainResponse[]; }>(appApiPath(`/apps/${serializePathParameter(appId, { name: 'appId', style: 'simple', explode: false })}/domains`), { ...(requestOptions?.signal !== undefined ? { signal: requestOptions.signal } : {}), ...(requestOptions?.timeout !== undefined ? { timeout: requestOptions.timeout } : {}), method: 'GET' as any, sdkworkUnwrapKind: 'data' });
  }
}

export class AppReadinessApi {
  private client: HttpClient;

  constructor(client: HttpClient) {
    this.client = client;
  }


/** Derived application factory readiness */
  async retrieve(appId: string, requestOptions?: ApiRequestOptions): Promise<AppFactoryReadiness> {
    return this.client.request<AppFactoryReadiness>(appApiPath(`/apps/${serializePathParameter(appId, { name: 'appId', style: 'simple', explode: false })}/readiness`), { ...(requestOptions?.signal !== undefined ? { signal: requestOptions.signal } : {}), ...(requestOptions?.timeout !== undefined ? { timeout: requestOptions.timeout } : {}), method: 'GET' as any, sdkworkUnwrapKind: 'item' });
  }
}

export interface AppListParams {
  page?: number;
  pageSize?: number;
  keyword?: string;
  ownershipScope?: AppOwnerType;
  scope?: AppOwnerType;
  ownerOrganizationId?: string;
  appKind?: AppKind[];
}

export interface AppCreateParams {
  idempotencyKey: string;
}

export interface AppUninstallParams {
  idempotencyKey: string;
}

export interface AppActivateParams {
  idempotencyKey: string;
}

export interface AppPauseParams {
  idempotencyKey: string;
}

export class AppApi {
  private client: HttpClient;
  public readonly readiness: AppReadinessApi;
  public readonly domains: AppDomainsApi;
  public readonly composition: AppCompositionApi;
  public readonly sourceSpecs: AppSourceSpecsApi;
  public readonly source: AppSourceApi;
  public readonly remix: AppRemixApi;
  public readonly resourceLinks: AppResourceLinksApi;
  public readonly platformTargets: AppPlatformTargetsApi;
  public readonly sourceRepositories: AppSourceRepositoriesApi;
  public readonly scaffolds: AppScaffoldsApi;
  public readonly marketplace: AppMarketplaceApi;

  constructor(client: HttpClient) {
    this.client = client;
    this.readiness = new AppReadinessApi(client);
    this.domains = new AppDomainsApi(client);
    this.composition = new AppCompositionApi(client);
    this.sourceSpecs = new AppSourceSpecsApi(client);
    this.source = new AppSourceApi(client);
    this.remix = new AppRemixApi(client);
    this.resourceLinks = new AppResourceLinksApi(client);
    this.platformTargets = new AppPlatformTargetsApi(client);
    this.sourceRepositories = new AppSourceRepositoriesApi(client);
    this.scaffolds = new AppScaffoldsApi(client);
    this.marketplace = new AppMarketplaceApi(client);
  }


/** List the apps the caller may reach */
  async list(params?: AppListParams, requestOptions?: ApiRequestOptions): Promise<{ items: AppResponse[]; pageInfo: PageInfo; }> {
    const query = buildQueryString([
      { name: 'page', value: params?.page, style: 'form', explode: true, allowReserved: false },
      { name: 'page_size', value: params?.pageSize, style: 'form', explode: true, allowReserved: false },
      { name: 'keyword', value: params?.keyword, style: 'form', explode: true, allowReserved: false },
      { name: 'ownership_scope', value: params?.ownershipScope, style: 'form', explode: true, allowReserved: false },
      { name: 'scope', value: params?.scope, style: 'form', explode: true, allowReserved: false },
      { name: 'owner_organization_id', value: params?.ownerOrganizationId, style: 'form', explode: true, allowReserved: false },
      { name: 'app_kind', value: params?.appKind, style: 'form', explode: true, allowReserved: false },
    ]);
    return this.client.request<{ items: AppResponse[]; pageInfo: PageInfo; }>(appendQueryString(appApiPath(`/apps`), query), { ...(requestOptions?.signal !== undefined ? { signal: requestOptions.signal } : {}), ...(requestOptions?.timeout !== undefined ? { timeout: requestOptions.timeout } : {}), method: 'GET' as any, sdkworkUnwrapKind: 'page' });
  }

/** Create a tenant app */
  async create(body: CreateAppRequest, params: AppCreateParams, requestOptions?: ApiRequestOptions): Promise<AppResponse> {
    const requestHeaders = buildRequestHeaders(
      {
        'Idempotency-Key': { value: params.idempotencyKey, style: 'simple', explode: false },
      },
      {}
    );
    return this.client.request<AppResponse>(appApiPath(`/apps`), { ...(requestOptions?.signal !== undefined ? { signal: requestOptions.signal } : {}), ...(requestOptions?.timeout !== undefined ? { timeout: requestOptions.timeout } : {}), method: 'POST' as any, body, contentType: 'application/json', ...(requestHeaders !== undefined ? { headers: requestHeaders } : {}), sdkworkUnwrapKind: 'item' });
  }

/** Forward-only application uninstall */
  async uninstall(appId: string, params: AppUninstallParams, requestOptions?: ApiRequestOptions): Promise<AppUninstallResponse> {
    const requestHeaders = buildRequestHeaders(
      {
        'Idempotency-Key': { value: params.idempotencyKey, style: 'simple', explode: false },
      },
      {}
    );
    return this.client.request<AppUninstallResponse>(appApiPath(`/apps/${serializePathParameter(appId, { name: 'appId', style: 'simple', explode: false })}/uninstall`), { ...(requestOptions?.signal !== undefined ? { signal: requestOptions.signal } : {}), ...(requestOptions?.timeout !== undefined ? { timeout: requestOptions.timeout } : {}), method: 'POST' as any, ...(requestHeaders !== undefined ? { headers: requestHeaders } : {}), sdkworkUnwrapKind: 'item' });
  }

/** Retrieve a tenant app */
  async retrieve(appId: string, requestOptions?: ApiRequestOptions): Promise<AppResponse> {
    return this.client.request<AppResponse>(appApiPath(`/apps/${serializePathParameter(appId, { name: 'appId', style: 'simple', explode: false })}`), { ...(requestOptions?.signal !== undefined ? { signal: requestOptions.signal } : {}), ...(requestOptions?.timeout !== undefined ? { timeout: requestOptions.timeout } : {}), method: 'GET' as any, sdkworkUnwrapKind: 'item' });
  }

/** Update a tenant app */
  async update(appId: string, body: UpdateAppRequest, requestOptions?: ApiRequestOptions): Promise<AppResponse> {
    return this.client.request<AppResponse>(appApiPath(`/apps/${serializePathParameter(appId, { name: 'appId', style: 'simple', explode: false })}`), { ...(requestOptions?.signal !== undefined ? { signal: requestOptions.signal } : {}), ...(requestOptions?.timeout !== undefined ? { timeout: requestOptions.timeout } : {}), method: 'PATCH' as any, body, contentType: 'application/json', sdkworkUnwrapKind: 'item' });
  }

/** 激活应用 */
  async activate(appId: string, params: AppActivateParams, requestOptions?: ApiRequestOptions): Promise<AppResponse> {
    const requestHeaders = buildRequestHeaders(
      {
        'Idempotency-Key': { value: params.idempotencyKey, style: 'simple', explode: false },
      },
      {}
    );
    return this.client.request<AppResponse>(appApiPath(`/apps/${serializePathParameter(appId, { name: 'appId', style: 'simple', explode: false })}/activate`), { ...(requestOptions?.signal !== undefined ? { signal: requestOptions.signal } : {}), ...(requestOptions?.timeout !== undefined ? { timeout: requestOptions.timeout } : {}), method: 'POST' as any, ...(requestHeaders !== undefined ? { headers: requestHeaders } : {}), sdkworkUnwrapKind: 'item' });
  }

/** 暂停应用 */
  async pause(appId: string, params: AppPauseParams, requestOptions?: ApiRequestOptions): Promise<AppResponse> {
    const requestHeaders = buildRequestHeaders(
      {
        'Idempotency-Key': { value: params.idempotencyKey, style: 'simple', explode: false },
      },
      {}
    );
    return this.client.request<AppResponse>(appApiPath(`/apps/${serializePathParameter(appId, { name: 'appId', style: 'simple', explode: false })}/pause`), { ...(requestOptions?.signal !== undefined ? { signal: requestOptions.signal } : {}), ...(requestOptions?.timeout !== undefined ? { timeout: requestOptions.timeout } : {}), method: 'POST' as any, ...(requestHeaders !== undefined ? { headers: requestHeaders } : {}), sdkworkUnwrapKind: 'item' });
  }
}

export function createAppApi(client: HttpClient): AppApi {
  return new AppApi(client);
}

function appendQueryString(path: string, rawQueryString: string): string {
  const query = rawQueryString.replace(/^\?+/, '');
  if (!query) {
    return path;
  }
  return path.includes('?') ? `${path}&${query}` : `${path}?${query}`;
}

interface PathParameterSpec {
  name: string;
  style: string;
  explode: boolean;
}

function serializePathParameter(value: unknown, spec: PathParameterSpec): string {
  if (value === undefined || value === null) {
    return '';
  }

  const style = spec.style || 'simple';
  if (Array.isArray(value)) {
    return serializePathArray(spec.name, value, style, spec.explode);
  }
  if (typeof value === 'object') {
    return serializePathObject(spec.name, value as Record<string, unknown>, style, spec.explode);
  }
  return pathPrefix(spec.name, style, false) + encodePathValue(serializePathPrimitive(value));
}

function serializePathArray(name: string, values: unknown[], style: string, explode: boolean): string {
  const serialized = values
    .filter((item) => item !== undefined && item !== null)
    .map((item) => encodePathValue(serializePathPrimitive(item)));
  if (serialized.length === 0) {
    return pathPrefix(name, style, false);
  }
  if (style === 'matrix') {
    return explode
      ? serialized.map((item) => `;${name}=${item}`).join('')
      : `;${name}=${serialized.join(',')}`;
  }
  return pathPrefix(name, style, false) + serialized.join(explode ? '.' : ',');
}

function serializePathObject(name: string, value: Record<string, unknown>, style: string, explode: boolean): string {
  const entries = Object.entries(value).filter(([, entryValue]) => entryValue !== undefined && entryValue !== null);
  if (entries.length === 0) {
    return pathPrefix(name, style, true);
  }
  if (style === 'matrix') {
    return explode
      ? entries.map(([key, entryValue]) => `;${encodePathValue(key)}=${encodePathValue(serializePathPrimitive(entryValue))}`).join('')
      : `;${name}=${entries.flatMap(([key, entryValue]) => [encodePathValue(key), encodePathValue(serializePathPrimitive(entryValue))]).join(',')}`;
  }
  const serialized = explode
    ? entries.map(([key, entryValue]) => `${encodePathValue(key)}=${encodePathValue(serializePathPrimitive(entryValue))}`).join(style === 'label' ? '.' : ',')
    : entries.flatMap(([key, entryValue]) => [encodePathValue(key), encodePathValue(serializePathPrimitive(entryValue))]).join(',');
  return pathPrefix(name, style, true) + serialized;
}

function pathPrefix(name: string, style: string, _objectValue: boolean): string {
  if (style === 'label') return '.';
  if (style === 'matrix') return `;${name}`;
  return '';
}

function encodePathValue(value: string): string {
  return encodeURIComponent(value);
}

function serializePathPrimitive(value: unknown): string {
  if (value instanceof Date) {
    return value.toISOString();
  }
  if (typeof value === 'object') {
    return JSON.stringify(value);
  }
  return String(value);
}
interface QueryParameterSpec {
  name: string;
  value: unknown;
  style: string;
  explode: boolean;
  allowReserved: boolean;
  contentType?: string;
}

function buildQueryString(parameters: QueryParameterSpec[]): string {
  const pairs: string[] = [];
  for (const parameter of parameters) {
    appendSerializedParameter(pairs, parameter);
  }
  return pairs.join('&');
}

function appendSerializedParameter(pairs: string[], parameter: QueryParameterSpec): void {
  if (parameter.value === undefined || parameter.value === null) {
    return;
  }

  if (parameter.contentType) {
    pairs.push(`${encodeQueryComponent(parameter.name)}=${encodeQueryValue(JSON.stringify(parameter.value), parameter.allowReserved)}`);
    return;
  }

  const style = parameter.style || 'form';
  if (style === 'deepObject') {
    appendDeepObjectParameter(pairs, parameter.name, parameter.value, parameter.allowReserved);
    return;
  }

  if (Array.isArray(parameter.value)) {
    appendArrayParameter(pairs, parameter.name, parameter.value, style, parameter.explode, parameter.allowReserved);
    return;
  }

  if (typeof parameter.value === 'object') {
    appendObjectParameter(pairs, parameter.name, parameter.value as Record<string, unknown>, style, parameter.explode, parameter.allowReserved);
    return;
  }

  pairs.push(`${encodeQueryComponent(parameter.name)}=${encodeQueryValue(serializePrimitive(parameter.value), parameter.allowReserved)}`);
}

function appendArrayParameter(
  pairs: string[],
  name: string,
  value: unknown[],
  style: string,
  explode: boolean,
  allowReserved: boolean,
): void {
  const values = value
    .filter((item) => item !== undefined && item !== null)
    .map((item) => serializePrimitive(item));
  if (values.length === 0) {
    return;
  }

  if (style === 'form' && explode) {
    for (const item of values) {
      pairs.push(`${encodeQueryComponent(name)}=${encodeQueryValue(item, allowReserved)}`);
    }
    return;
  }

  pairs.push(`${encodeQueryComponent(name)}=${encodeQueryValue(values.join(','), allowReserved)}`);
}

function appendObjectParameter(
  pairs: string[],
  name: string,
  value: Record<string, unknown>,
  style: string,
  explode: boolean,
  allowReserved: boolean,
): void {
  const entries = Object.entries(value).filter(([, entryValue]) => entryValue !== undefined && entryValue !== null);
  if (entries.length === 0) {
    return;
  }

  if (style === 'form' && explode) {
    for (const [key, entryValue] of entries) {
      pairs.push(`${encodeQueryComponent(key)}=${encodeQueryValue(serializePrimitive(entryValue), allowReserved)}`);
    }
    return;
  }

  const serialized = entries.flatMap(([key, entryValue]) => [key, serializePrimitive(entryValue)]).join(',');
  pairs.push(`${encodeQueryComponent(name)}=${encodeQueryValue(serialized, allowReserved)}`);
}

function appendDeepObjectParameter(
  pairs: string[],
  name: string,
  value: unknown,
  allowReserved: boolean,
): void {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    pairs.push(`${encodeQueryComponent(name)}=${encodeQueryValue(serializePrimitive(value), allowReserved)}`);
    return;
  }

  for (const [key, entryValue] of Object.entries(value as Record<string, unknown>)) {
    if (entryValue === undefined || entryValue === null) {
      continue;
    }
    pairs.push(`${encodeQueryComponent(`${name}[${key}]`)}=${encodeQueryValue(serializePrimitive(entryValue), allowReserved)}`);
  }
}

function serializePrimitive(value: unknown): string {
  if (value instanceof Date) {
    return value.toISOString();
  }
  if (typeof value === 'object') {
    return JSON.stringify(value);
  }
  return String(value);
}

function encodeQueryComponent(value: string): string {
  return encodeURIComponent(value);
}

function encodeQueryValue(value: string, allowReserved: boolean): string {
  const encoded = encodeURIComponent(value);
  if (!allowReserved) {
    return encoded;
  }
  return encoded.replace(/%3A/gi, ':')
    .replace(/%2F/gi, '/')
    .replace(/%3F/gi, '?')
    .replace(/%23/gi, '#')
    .replace(/%5B/gi, '[')
    .replace(/%5D/gi, ']')
    .replace(/%40/gi, '@')
    .replace(/%21/gi, '!')
    .replace(/%24/gi, '$')
    .replace(/%26/gi, '&')
    .replace(/%27/gi, "'")
    .replace(/%28/gi, '(')
    .replace(/%29/gi, ')')
    .replace(/%2A/gi, '*')
    .replace(/%2B/gi, '+')
    .replace(/%2C/gi, ',')
    .replace(/%3B/gi, ';')
    .replace(/%3D/gi, '=');
}
function buildRequestHeaders(
  headers: Record<string, HeaderParameterSpec | undefined>,
  cookies: Record<string, HeaderParameterSpec | undefined> = {},
): Record<string, string> | undefined {
  const requestHeaders: Record<string, string> = {};

  for (const [name, parameter] of Object.entries(headers)) {
    const serialized = serializeParameterValue(parameter);
    if (serialized !== undefined) {
      requestHeaders[name] = serialized;
    }
  }

  const cookieHeader = buildCookieHeader(cookies);
  if (cookieHeader) {
    requestHeaders.Cookie = requestHeaders.Cookie
      ? `${requestHeaders.Cookie}; ${cookieHeader}`
      : cookieHeader;
  }

  return Object.keys(requestHeaders).length > 0 ? requestHeaders : undefined;
}

interface HeaderParameterSpec {
  value: unknown;
  style: string;
  explode: boolean;
  contentType?: string;
}

function buildCookieHeader(cookies: Record<string, HeaderParameterSpec | undefined>): string | undefined {
  const pairs: string[] = [];
  for (const [name, parameter] of Object.entries(cookies)) {
    const serialized = serializeParameterValue(parameter);
    if (serialized !== undefined) {
      pairs.push(`${encodeURIComponent(name)}=${encodeURIComponent(serialized)}`);
    }
  }
  return pairs.length > 0 ? pairs.join('; ') : undefined;
}

function serializeParameterValue(parameter: HeaderParameterSpec | undefined): string | undefined {
  const value = parameter?.value;
  if (value === undefined || value === null) {
    return undefined;
  }
  if (parameter?.contentType) {
    return JSON.stringify(value);
  }
  if (value instanceof Date) {
    return value.toISOString();
  }
  if (Array.isArray(value)) {
    return value.map((item) => serializeHeaderPrimitive(item)).join(',');
  }
  if (typeof value === 'object' && value !== null) {
    return serializeHeaderObject(value as Record<string, unknown>, parameter?.explode === true);
  }
  return serializeHeaderPrimitive(value);
}

function serializeHeaderObject(value: Record<string, unknown>, explode: boolean): string {
  const entries = Object.entries(value).filter(([, entryValue]) => entryValue !== undefined && entryValue !== null);
  if (explode) {
    return entries.map(([key, entryValue]) => `${key}=${serializeHeaderPrimitive(entryValue)}`).join(',');
  }
  return entries.flatMap(([key, entryValue]) => [key, serializeHeaderPrimitive(entryValue)]).join(',');
}

function serializeHeaderPrimitive(value: unknown): string {
  if (value instanceof Date) {
    return value.toISOString();
  }
  return String(value);
}
