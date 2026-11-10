import { backendApiPath } from './paths';
import type { ApiRequestOptions, HttpClient } from '../http/client';

import type { DownloadCardResponse, DownloadCardUpsertRequest, DownloadCatalogResponse, DownloadCatalogSummaryResponse, DownloadCatalogUpsertRequest, DownloadItemResponse, DownloadItemUpsertRequest, DownloadUpdateResponse, DownloadUpdateUpsertRequest } from '../types';


export class DownloadCatalogDownloadUpdateApi {
  private client: HttpClient;

  constructor(client: HttpClient) {
    this.client = client;
  }


/** Create or replace one installer's update configuration */
  async upsert(catalogKey: string, cardKey: string, itemKey: string, body: DownloadUpdateUpsertRequest, requestOptions?: ApiRequestOptions): Promise<DownloadUpdateResponse> {
    return this.client.request<DownloadUpdateResponse>(backendApiPath(`/downloads/catalogs/${serializePathParameter(catalogKey, { name: 'catalogKey', style: 'simple', explode: false })}/cards/${serializePathParameter(cardKey, { name: 'cardKey', style: 'simple', explode: false })}/items/${serializePathParameter(itemKey, { name: 'itemKey', style: 'simple', explode: false })}/update`), { ...(requestOptions?.signal !== undefined ? { signal: requestOptions.signal } : {}), ...(requestOptions?.timeout !== undefined ? { timeout: requestOptions.timeout } : {}), method: 'PUT' as any, body, contentType: 'application/json', sdkworkUnwrapKind: 'data' });
  }
}

export class DownloadCatalogDownloadItemApi {
  private client: HttpClient;

  constructor(client: HttpClient) {
    this.client = client;
  }


/** Create or patch one installer of a card */
  async upsert(catalogKey: string, cardKey: string, itemKey: string, body: DownloadItemUpsertRequest, requestOptions?: ApiRequestOptions): Promise<DownloadItemResponse> {
    return this.client.request<DownloadItemResponse>(backendApiPath(`/downloads/catalogs/${serializePathParameter(catalogKey, { name: 'catalogKey', style: 'simple', explode: false })}/cards/${serializePathParameter(cardKey, { name: 'cardKey', style: 'simple', explode: false })}/items/${serializePathParameter(itemKey, { name: 'itemKey', style: 'simple', explode: false })}`), { ...(requestOptions?.signal !== undefined ? { signal: requestOptions.signal } : {}), ...(requestOptions?.timeout !== undefined ? { timeout: requestOptions.timeout } : {}), method: 'PUT' as any, body, contentType: 'application/json', sdkworkUnwrapKind: 'data' });
  }

/** Delete one installer and its update configuration */
  async delete(catalogKey: string, cardKey: string, itemKey: string, requestOptions?: ApiRequestOptions): Promise<void> {
    return this.client.request<void>(backendApiPath(`/downloads/catalogs/${serializePathParameter(catalogKey, { name: 'catalogKey', style: 'simple', explode: false })}/cards/${serializePathParameter(cardKey, { name: 'cardKey', style: 'simple', explode: false })}/items/${serializePathParameter(itemKey, { name: 'itemKey', style: 'simple', explode: false })}`), { ...(requestOptions?.signal !== undefined ? { signal: requestOptions.signal } : {}), ...(requestOptions?.timeout !== undefined ? { timeout: requestOptions.timeout } : {}), method: 'DELETE' as any });
  }
}

export class DownloadCatalogDownloadCardApi {
  private client: HttpClient;

  constructor(client: HttpClient) {
    this.client = client;
  }


/** Create or patch one card of a download catalog */
  async upsert(catalogKey: string, cardKey: string, body: DownloadCardUpsertRequest, requestOptions?: ApiRequestOptions): Promise<DownloadCardResponse> {
    return this.client.request<DownloadCardResponse>(backendApiPath(`/downloads/catalogs/${serializePathParameter(catalogKey, { name: 'catalogKey', style: 'simple', explode: false })}/cards/${serializePathParameter(cardKey, { name: 'cardKey', style: 'simple', explode: false })}`), { ...(requestOptions?.signal !== undefined ? { signal: requestOptions.signal } : {}), ...(requestOptions?.timeout !== undefined ? { timeout: requestOptions.timeout } : {}), method: 'PUT' as any, body, contentType: 'application/json', sdkworkUnwrapKind: 'data' });
  }

/** Delete one card and its installers */
  async delete(catalogKey: string, cardKey: string, requestOptions?: ApiRequestOptions): Promise<void> {
    return this.client.request<void>(backendApiPath(`/downloads/catalogs/${serializePathParameter(catalogKey, { name: 'catalogKey', style: 'simple', explode: false })}/cards/${serializePathParameter(cardKey, { name: 'cardKey', style: 'simple', explode: false })}`), { ...(requestOptions?.signal !== undefined ? { signal: requestOptions.signal } : {}), ...(requestOptions?.timeout !== undefined ? { timeout: requestOptions.timeout } : {}), method: 'DELETE' as any });
  }
}

export class DownloadCatalogApi {
  private client: HttpClient;
  public readonly downloadCard: DownloadCatalogDownloadCardApi;
  public readonly downloadItem: DownloadCatalogDownloadItemApi;
  public readonly downloadUpdate: DownloadCatalogDownloadUpdateApi;

  constructor(client: HttpClient) {
    this.client = client;
    this.downloadCard = new DownloadCatalogDownloadCardApi(client);
    this.downloadItem = new DownloadCatalogDownloadItemApi(client);
    this.downloadUpdate = new DownloadCatalogDownloadUpdateApi(client);
  }


/** List download catalogs */
  async list(requestOptions?: ApiRequestOptions): Promise<DownloadCatalogSummaryResponse[]> {
    return this.client.request<DownloadCatalogSummaryResponse[]>(backendApiPath(`/downloads/catalogs`), { ...(requestOptions?.signal !== undefined ? { signal: requestOptions.signal } : {}), ...(requestOptions?.timeout !== undefined ? { timeout: requestOptions.timeout } : {}), method: 'GET' as any, sdkworkUnwrapKind: 'data' });
  }

/** Read one download catalog with cards, installers and update configurations */
  async retrieve(catalogKey: string, requestOptions?: ApiRequestOptions): Promise<DownloadCatalogResponse> {
    return this.client.request<DownloadCatalogResponse>(backendApiPath(`/downloads/catalogs/${serializePathParameter(catalogKey, { name: 'catalogKey', style: 'simple', explode: false })}`), { ...(requestOptions?.signal !== undefined ? { signal: requestOptions.signal } : {}), ...(requestOptions?.timeout !== undefined ? { timeout: requestOptions.timeout } : {}), method: 'GET' as any, sdkworkUnwrapKind: 'data' });
  }

/** Create or patch a download catalog */
  async upsert(catalogKey: string, body: DownloadCatalogUpsertRequest, requestOptions?: ApiRequestOptions): Promise<DownloadCatalogResponse> {
    return this.client.request<DownloadCatalogResponse>(backendApiPath(`/downloads/catalogs/${serializePathParameter(catalogKey, { name: 'catalogKey', style: 'simple', explode: false })}`), { ...(requestOptions?.signal !== undefined ? { signal: requestOptions.signal } : {}), ...(requestOptions?.timeout !== undefined ? { timeout: requestOptions.timeout } : {}), method: 'PUT' as any, body, contentType: 'application/json', sdkworkUnwrapKind: 'data' });
  }

/** Publish a download catalog and bump its revision */
  async publish(catalogKey: string, requestOptions?: ApiRequestOptions): Promise<DownloadCatalogResponse> {
    return this.client.request<DownloadCatalogResponse>(backendApiPath(`/downloads/catalogs/${serializePathParameter(catalogKey, { name: 'catalogKey', style: 'simple', explode: false })}/publish`), { ...(requestOptions?.signal !== undefined ? { signal: requestOptions.signal } : {}), ...(requestOptions?.timeout !== undefined ? { timeout: requestOptions.timeout } : {}), method: 'POST' as any, sdkworkUnwrapKind: 'data' });
  }
}

export function createDownloadCatalogApi(client: HttpClient): DownloadCatalogApi {
  return new DownloadCatalogApi(client);
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
