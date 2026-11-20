import { backendApiPath } from './paths';
import type { ApiRequestOptions, HttpClient } from '../http/client';

import type { DownloadCardResponse, DownloadCardUpsertRequest, DownloadCatalogResponse, DownloadCatalogSummaryResponse, DownloadCatalogUpsertRequest, DownloadItemResponse, DownloadItemUpsertRequest, DownloadUpdateResponse, DownloadUpdateUpsertRequest } from '../types';


export interface DownloadsDownloadUpdateUpdateParams {
  idempotencyKey: string;
}

export class DownloadsDownloadUpdateApi {
  private client: HttpClient;

  constructor(client: HttpClient) {
    this.client = client;
  }


/** Create or replace one installer's update configuration */
  async update(catalogKey: string, cardKey: string, itemKey: string, body: DownloadUpdateUpsertRequest, params: DownloadsDownloadUpdateUpdateParams, requestOptions?: ApiRequestOptions): Promise<DownloadUpdateResponse> {
    const requestHeaders = buildRequestHeaders(
      {
        'Idempotency-Key': { value: params.idempotencyKey, style: 'simple', explode: false },
      },
      {}
    );
    return this.client.request<DownloadUpdateResponse>(backendApiPath(`/downloads/catalogs/${serializePathParameter(catalogKey, { name: 'catalogKey', style: 'simple', explode: false })}/cards/${serializePathParameter(cardKey, { name: 'cardKey', style: 'simple', explode: false })}/items/${serializePathParameter(itemKey, { name: 'itemKey', style: 'simple', explode: false })}/update`), { ...(requestOptions?.signal !== undefined ? { signal: requestOptions.signal } : {}), ...(requestOptions?.timeout !== undefined ? { timeout: requestOptions.timeout } : {}), method: 'PUT' as any, body, contentType: 'application/json', ...(requestHeaders !== undefined ? { headers: requestHeaders } : {}), sdkworkUnwrapKind: 'data' });
  }
}

export interface DownloadsDownloadItemUpdateParams {
  idempotencyKey: string;
}

export class DownloadsDownloadItemApi {
  private client: HttpClient;

  constructor(client: HttpClient) {
    this.client = client;
  }


/** Create or patch one installer of a card */
  async update(catalogKey: string, cardKey: string, itemKey: string, body: DownloadItemUpsertRequest, params: DownloadsDownloadItemUpdateParams, requestOptions?: ApiRequestOptions): Promise<DownloadItemResponse> {
    const requestHeaders = buildRequestHeaders(
      {
        'Idempotency-Key': { value: params.idempotencyKey, style: 'simple', explode: false },
      },
      {}
    );
    return this.client.request<DownloadItemResponse>(backendApiPath(`/downloads/catalogs/${serializePathParameter(catalogKey, { name: 'catalogKey', style: 'simple', explode: false })}/cards/${serializePathParameter(cardKey, { name: 'cardKey', style: 'simple', explode: false })}/items/${serializePathParameter(itemKey, { name: 'itemKey', style: 'simple', explode: false })}`), { ...(requestOptions?.signal !== undefined ? { signal: requestOptions.signal } : {}), ...(requestOptions?.timeout !== undefined ? { timeout: requestOptions.timeout } : {}), method: 'PUT' as any, body, contentType: 'application/json', ...(requestHeaders !== undefined ? { headers: requestHeaders } : {}), sdkworkUnwrapKind: 'data' });
  }

/** Delete one installer and its update configuration */
  async delete(catalogKey: string, cardKey: string, itemKey: string, requestOptions?: ApiRequestOptions): Promise<void> {
    return this.client.request<void>(backendApiPath(`/downloads/catalogs/${serializePathParameter(catalogKey, { name: 'catalogKey', style: 'simple', explode: false })}/cards/${serializePathParameter(cardKey, { name: 'cardKey', style: 'simple', explode: false })}/items/${serializePathParameter(itemKey, { name: 'itemKey', style: 'simple', explode: false })}`), { ...(requestOptions?.signal !== undefined ? { signal: requestOptions.signal } : {}), ...(requestOptions?.timeout !== undefined ? { timeout: requestOptions.timeout } : {}), method: 'DELETE' as any });
  }
}

export interface DownloadsDownloadCardUpdateParams {
  idempotencyKey: string;
}

export class DownloadsDownloadCardApi {
  private client: HttpClient;

  constructor(client: HttpClient) {
    this.client = client;
  }


/** Create or patch one card of a download catalog */
  async update(catalogKey: string, cardKey: string, body: DownloadCardUpsertRequest, params: DownloadsDownloadCardUpdateParams, requestOptions?: ApiRequestOptions): Promise<DownloadCardResponse> {
    const requestHeaders = buildRequestHeaders(
      {
        'Idempotency-Key': { value: params.idempotencyKey, style: 'simple', explode: false },
      },
      {}
    );
    return this.client.request<DownloadCardResponse>(backendApiPath(`/downloads/catalogs/${serializePathParameter(catalogKey, { name: 'catalogKey', style: 'simple', explode: false })}/cards/${serializePathParameter(cardKey, { name: 'cardKey', style: 'simple', explode: false })}`), { ...(requestOptions?.signal !== undefined ? { signal: requestOptions.signal } : {}), ...(requestOptions?.timeout !== undefined ? { timeout: requestOptions.timeout } : {}), method: 'PUT' as any, body, contentType: 'application/json', ...(requestHeaders !== undefined ? { headers: requestHeaders } : {}), sdkworkUnwrapKind: 'data' });
  }

/** Delete one card and its installers */
  async delete(catalogKey: string, cardKey: string, requestOptions?: ApiRequestOptions): Promise<void> {
    return this.client.request<void>(backendApiPath(`/downloads/catalogs/${serializePathParameter(catalogKey, { name: 'catalogKey', style: 'simple', explode: false })}/cards/${serializePathParameter(cardKey, { name: 'cardKey', style: 'simple', explode: false })}`), { ...(requestOptions?.signal !== undefined ? { signal: requestOptions.signal } : {}), ...(requestOptions?.timeout !== undefined ? { timeout: requestOptions.timeout } : {}), method: 'DELETE' as any });
  }
}

export interface DownloadsDownloadCatalogUpdateParams {
  idempotencyKey: string;
}

export class DownloadsDownloadCatalogApi {
  private client: HttpClient;

  constructor(client: HttpClient) {
    this.client = client;
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
  async update(catalogKey: string, body: DownloadCatalogUpsertRequest, params: DownloadsDownloadCatalogUpdateParams, requestOptions?: ApiRequestOptions): Promise<DownloadCatalogResponse> {
    const requestHeaders = buildRequestHeaders(
      {
        'Idempotency-Key': { value: params.idempotencyKey, style: 'simple', explode: false },
      },
      {}
    );
    return this.client.request<DownloadCatalogResponse>(backendApiPath(`/downloads/catalogs/${serializePathParameter(catalogKey, { name: 'catalogKey', style: 'simple', explode: false })}`), { ...(requestOptions?.signal !== undefined ? { signal: requestOptions.signal } : {}), ...(requestOptions?.timeout !== undefined ? { timeout: requestOptions.timeout } : {}), method: 'PUT' as any, body, contentType: 'application/json', ...(requestHeaders !== undefined ? { headers: requestHeaders } : {}), sdkworkUnwrapKind: 'data' });
  }

/** Publish a download catalog and bump its revision */
  async publish(catalogKey: string, requestOptions?: ApiRequestOptions): Promise<DownloadCatalogResponse> {
    return this.client.request<DownloadCatalogResponse>(backendApiPath(`/downloads/catalogs/${serializePathParameter(catalogKey, { name: 'catalogKey', style: 'simple', explode: false })}/publish`), { ...(requestOptions?.signal !== undefined ? { signal: requestOptions.signal } : {}), ...(requestOptions?.timeout !== undefined ? { timeout: requestOptions.timeout } : {}), method: 'POST' as any, sdkworkUnwrapKind: 'data' });
  }
}

export class DownloadsApi {
  public readonly downloadCatalog: DownloadsDownloadCatalogApi;
  public readonly downloadCard: DownloadsDownloadCardApi;
  public readonly downloadItem: DownloadsDownloadItemApi;
  public readonly downloadUpdate: DownloadsDownloadUpdateApi;

  constructor(client: HttpClient) {
    this.downloadCatalog = new DownloadsDownloadCatalogApi(client);
    this.downloadCard = new DownloadsDownloadCardApi(client);
    this.downloadItem = new DownloadsDownloadItemApi(client);
    this.downloadUpdate = new DownloadsDownloadUpdateApi(client);
  }

}

export function createDownloadsApi(client: HttpClient): DownloadsApi {
  return new DownloadsApi(client);
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
