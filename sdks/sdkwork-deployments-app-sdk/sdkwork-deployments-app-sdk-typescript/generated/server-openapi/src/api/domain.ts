import { appApiPath } from './paths';
import type { ApiRequestOptions, HttpClient } from '../http/client';

import type { CloudAccountRegistrationResponse, CloudAccountResponse, CreateCloudAccountRequest, CreateDomainHostnameRequest, CreateDomainZoneRequest, DomainHostnameClaimResponse, DomainHostnameResponse, DomainVerifyResponse, DomainZoneResponse, EnsureDomainHostnameClaimsRequest, PageInfo, UpdateDomainHostnameRequest, UpdateDomainZoneRequest } from '../types';


export interface DomainCloudAccountsListParams {
  page?: number;
  pageSize?: number;
  dnsProvider?: 'ALIYUN_DNS' | 'DNSPOD' | 'CLOUDFLARE';
  scopeType?: 'platform' | 'tenant' | 'user';
  mine?: boolean;
  keyword?: string;
}

export interface DomainCloudAccountsCreateParams {
  idempotencyKey: string;
}

export class DomainCloudAccountsApi {
  private client: HttpClient;

  constructor(client: HttpClient) {
    this.client = client;
  }


/** List cloud accounts usable for DNS automation */
  async list(params?: DomainCloudAccountsListParams, requestOptions?: ApiRequestOptions): Promise<{ items: CloudAccountResponse[]; pageInfo: PageInfo; }> {
    const query = buildQueryString([
      { name: 'page', value: params?.page, style: 'form', explode: true, allowReserved: false },
      { name: 'page_size', value: params?.pageSize, style: 'form', explode: true, allowReserved: false },
      { name: 'dnsProvider', value: params?.dnsProvider, style: 'form', explode: true, allowReserved: false },
      { name: 'scopeType', value: params?.scopeType, style: 'form', explode: true, allowReserved: false },
      { name: 'mine', value: params?.mine, style: 'form', explode: true, allowReserved: false },
      { name: 'keyword', value: params?.keyword, style: 'form', explode: true, allowReserved: false },
    ]);
    return this.client.request<{ items: CloudAccountResponse[]; pageInfo: PageInfo; }>(appendQueryString(appApiPath(`/cloud_accounts`), query), { ...(requestOptions?.signal !== undefined ? { signal: requestOptions.signal } : {}), ...(requestOptions?.timeout !== undefined ? { timeout: requestOptions.timeout } : {}), method: 'GET' as any, sdkworkUnwrapKind: 'page' });
  }

/** Register a cloud account for DNS automation */
  async create(body: CreateCloudAccountRequest, params: DomainCloudAccountsCreateParams, requestOptions?: ApiRequestOptions): Promise<CloudAccountRegistrationResponse> {
    const requestHeaders = buildRequestHeaders(
      {
        'Idempotency-Key': { value: params.idempotencyKey, style: 'simple', explode: false },
      },
      {}
    );
    return this.client.request<CloudAccountRegistrationResponse>(appApiPath(`/cloud_accounts`), { ...(requestOptions?.signal !== undefined ? { signal: requestOptions.signal } : {}), ...(requestOptions?.timeout !== undefined ? { timeout: requestOptions.timeout } : {}), method: 'POST' as any, body, contentType: 'application/json', ...(requestHeaders !== undefined ? { headers: requestHeaders } : {}), sdkworkUnwrapKind: 'item' });
  }
}

export interface DomainDomainZonesHostnameClaimsEnsureParams {
  idempotencyKey: string;
}

export class DomainDomainZonesHostnameClaimsApi {
  private client: HttpClient;

  constructor(client: HttpClient) {
    this.client = client;
  }


/** Ensure ownership of a set of hostnames */
  async ensure(zoneId: string, body: EnsureDomainHostnameClaimsRequest, params: DomainDomainZonesHostnameClaimsEnsureParams, requestOptions?: ApiRequestOptions): Promise<{ items: DomainHostnameClaimResponse[]; }> {
    const requestHeaders = buildRequestHeaders(
      {
        'Idempotency-Key': { value: params.idempotencyKey, style: 'simple', explode: false },
      },
      {}
    );
    return this.client.request<{ items: DomainHostnameClaimResponse[]; }>(appApiPath(`/domain_zones/${serializePathParameter(zoneId, { name: 'zoneId', style: 'simple', explode: false })}/hostname_claims`), { ...(requestOptions?.signal !== undefined ? { signal: requestOptions.signal } : {}), ...(requestOptions?.timeout !== undefined ? { timeout: requestOptions.timeout } : {}), method: 'POST' as any, body, contentType: 'application/json', ...(requestHeaders !== undefined ? { headers: requestHeaders } : {}), sdkworkUnwrapKind: 'data' });
  }
}

export interface DomainDomainZonesHostnamesListParams {
  page?: number;
  pageSize?: number;
}

export interface DomainDomainZonesHostnamesCreateParams {
  idempotencyKey: string;
}

export interface DomainDomainZonesHostnamesVerifyParams {
  idempotencyKey: string;
}

export class DomainDomainZonesHostnamesApi {
  private client: HttpClient;

  constructor(client: HttpClient) {
    this.client = client;
  }


/** List hostnames in a root domain zone */
  async list(zoneId: string, params?: DomainDomainZonesHostnamesListParams, requestOptions?: ApiRequestOptions): Promise<{ items: DomainHostnameResponse[]; pageInfo: PageInfo; }> {
    const query = buildQueryString([
      { name: 'page', value: params?.page, style: 'form', explode: true, allowReserved: false },
      { name: 'page_size', value: params?.pageSize, style: 'form', explode: true, allowReserved: false },
    ]);
    return this.client.request<{ items: DomainHostnameResponse[]; pageInfo: PageInfo; }>(appendQueryString(appApiPath(`/domain_zones/${serializePathParameter(zoneId, { name: 'zoneId', style: 'simple', explode: false })}/hostnames`), query), { ...(requestOptions?.signal !== undefined ? { signal: requestOptions.signal } : {}), ...(requestOptions?.timeout !== undefined ? { timeout: requestOptions.timeout } : {}), method: 'GET' as any, sdkworkUnwrapKind: 'page' });
  }

/** Create a hostname in a root domain zone */
  async create(zoneId: string, body: CreateDomainHostnameRequest, params: DomainDomainZonesHostnamesCreateParams, requestOptions?: ApiRequestOptions): Promise<DomainHostnameResponse> {
    const requestHeaders = buildRequestHeaders(
      {
        'Idempotency-Key': { value: params.idempotencyKey, style: 'simple', explode: false },
      },
      {}
    );
    return this.client.request<DomainHostnameResponse>(appApiPath(`/domain_zones/${serializePathParameter(zoneId, { name: 'zoneId', style: 'simple', explode: false })}/hostnames`), { ...(requestOptions?.signal !== undefined ? { signal: requestOptions.signal } : {}), ...(requestOptions?.timeout !== undefined ? { timeout: requestOptions.timeout } : {}), method: 'POST' as any, body, contentType: 'application/json', ...(requestHeaders !== undefined ? { headers: requestHeaders } : {}), sdkworkUnwrapKind: 'item' });
  }

/** Retrieve a domain hostname */
  async retrieve(zoneId: string, hostnameId: string, requestOptions?: ApiRequestOptions): Promise<DomainHostnameResponse> {
    return this.client.request<DomainHostnameResponse>(appApiPath(`/domain_zones/${serializePathParameter(zoneId, { name: 'zoneId', style: 'simple', explode: false })}/hostnames/${serializePathParameter(hostnameId, { name: 'hostnameId', style: 'simple', explode: false })}`), { ...(requestOptions?.signal !== undefined ? { signal: requestOptions.signal } : {}), ...(requestOptions?.timeout !== undefined ? { timeout: requestOptions.timeout } : {}), method: 'GET' as any, sdkworkUnwrapKind: 'item' });
  }

/** Rename a domain hostname */
  async update(zoneId: string, hostnameId: string, body: UpdateDomainHostnameRequest, requestOptions?: ApiRequestOptions): Promise<DomainHostnameResponse> {
    return this.client.request<DomainHostnameResponse>(appApiPath(`/domain_zones/${serializePathParameter(zoneId, { name: 'zoneId', style: 'simple', explode: false })}/hostnames/${serializePathParameter(hostnameId, { name: 'hostnameId', style: 'simple', explode: false })}`), { ...(requestOptions?.signal !== undefined ? { signal: requestOptions.signal } : {}), ...(requestOptions?.timeout !== undefined ? { timeout: requestOptions.timeout } : {}), method: 'PATCH' as any, body, contentType: 'application/json', sdkworkUnwrapKind: 'item' });
  }

/** Delete an unbound domain hostname */
  async delete(zoneId: string, hostnameId: string, requestOptions?: ApiRequestOptions): Promise<void> {
    return this.client.request<void>(appApiPath(`/domain_zones/${serializePathParameter(zoneId, { name: 'zoneId', style: 'simple', explode: false })}/hostnames/${serializePathParameter(hostnameId, { name: 'hostnameId', style: 'simple', explode: false })}`), { ...(requestOptions?.signal !== undefined ? { signal: requestOptions.signal } : {}), ...(requestOptions?.timeout !== undefined ? { timeout: requestOptions.timeout } : {}), method: 'DELETE' as any });
  }

/** Verify domain hostname ownership */
  async verify(zoneId: string, hostnameId: string, params: DomainDomainZonesHostnamesVerifyParams, requestOptions?: ApiRequestOptions): Promise<DomainVerifyResponse> {
    const requestHeaders = buildRequestHeaders(
      {
        'Idempotency-Key': { value: params.idempotencyKey, style: 'simple', explode: false },
      },
      {}
    );
    return this.client.request<DomainVerifyResponse>(appApiPath(`/domain_zones/${serializePathParameter(zoneId, { name: 'zoneId', style: 'simple', explode: false })}/hostnames/${serializePathParameter(hostnameId, { name: 'hostnameId', style: 'simple', explode: false })}/verify`), { ...(requestOptions?.signal !== undefined ? { signal: requestOptions.signal } : {}), ...(requestOptions?.timeout !== undefined ? { timeout: requestOptions.timeout } : {}), method: 'POST' as any, ...(requestHeaders !== undefined ? { headers: requestHeaders } : {}), sdkworkUnwrapKind: 'item' });
  }
}

export interface DomainDomainZonesListParams {
  page?: number;
  pageSize?: number;
  status?: 'ACTIVE' | 'PAUSED';
  keyword?: string;
}

export interface DomainDomainZonesCreateParams {
  idempotencyKey: string;
}

export class DomainDomainZonesApi {
  private client: HttpClient;
  public readonly hostnames: DomainDomainZonesHostnamesApi;
  public readonly hostnameClaims: DomainDomainZonesHostnameClaimsApi;

  constructor(client: HttpClient) {
    this.client = client;
    this.hostnames = new DomainDomainZonesHostnamesApi(client);
    this.hostnameClaims = new DomainDomainZonesHostnameClaimsApi(client);
  }


/** List root domain zones */
  async list(params?: DomainDomainZonesListParams, requestOptions?: ApiRequestOptions): Promise<{ items: DomainZoneResponse[]; pageInfo: PageInfo; }> {
    const query = buildQueryString([
      { name: 'page', value: params?.page, style: 'form', explode: true, allowReserved: false },
      { name: 'page_size', value: params?.pageSize, style: 'form', explode: true, allowReserved: false },
      { name: 'status', value: params?.status, style: 'form', explode: true, allowReserved: false },
      { name: 'keyword', value: params?.keyword, style: 'form', explode: true, allowReserved: false },
    ]);
    return this.client.request<{ items: DomainZoneResponse[]; pageInfo: PageInfo; }>(appendQueryString(appApiPath(`/domain_zones`), query), { ...(requestOptions?.signal !== undefined ? { signal: requestOptions.signal } : {}), ...(requestOptions?.timeout !== undefined ? { timeout: requestOptions.timeout } : {}), method: 'GET' as any, sdkworkUnwrapKind: 'page' });
  }

/** Define a root domain zone */
  async create(body: CreateDomainZoneRequest, params: DomainDomainZonesCreateParams, requestOptions?: ApiRequestOptions): Promise<DomainZoneResponse> {
    const requestHeaders = buildRequestHeaders(
      {
        'Idempotency-Key': { value: params.idempotencyKey, style: 'simple', explode: false },
      },
      {}
    );
    return this.client.request<DomainZoneResponse>(appApiPath(`/domain_zones`), { ...(requestOptions?.signal !== undefined ? { signal: requestOptions.signal } : {}), ...(requestOptions?.timeout !== undefined ? { timeout: requestOptions.timeout } : {}), method: 'POST' as any, body, contentType: 'application/json', ...(requestHeaders !== undefined ? { headers: requestHeaders } : {}), sdkworkUnwrapKind: 'item' });
  }

/** Retrieve a root domain zone */
  async retrieve(zoneId: string, requestOptions?: ApiRequestOptions): Promise<DomainZoneResponse> {
    return this.client.request<DomainZoneResponse>(appApiPath(`/domain_zones/${serializePathParameter(zoneId, { name: 'zoneId', style: 'simple', explode: false })}`), { ...(requestOptions?.signal !== undefined ? { signal: requestOptions.signal } : {}), ...(requestOptions?.timeout !== undefined ? { timeout: requestOptions.timeout } : {}), method: 'GET' as any, sdkworkUnwrapKind: 'item' });
  }

/** Update a root domain zone */
  async update(zoneId: string, body: UpdateDomainZoneRequest, requestOptions?: ApiRequestOptions): Promise<DomainZoneResponse> {
    return this.client.request<DomainZoneResponse>(appApiPath(`/domain_zones/${serializePathParameter(zoneId, { name: 'zoneId', style: 'simple', explode: false })}`), { ...(requestOptions?.signal !== undefined ? { signal: requestOptions.signal } : {}), ...(requestOptions?.timeout !== undefined ? { timeout: requestOptions.timeout } : {}), method: 'PATCH' as any, body, contentType: 'application/json', sdkworkUnwrapKind: 'item' });
  }

/** Delete an empty root domain zone */
  async delete(zoneId: string, requestOptions?: ApiRequestOptions): Promise<void> {
    return this.client.request<void>(appApiPath(`/domain_zones/${serializePathParameter(zoneId, { name: 'zoneId', style: 'simple', explode: false })}`), { ...(requestOptions?.signal !== undefined ? { signal: requestOptions.signal } : {}), ...(requestOptions?.timeout !== undefined ? { timeout: requestOptions.timeout } : {}), method: 'DELETE' as any });
  }
}

export class DomainApi {
  public readonly domainZones: DomainDomainZonesApi;
  public readonly cloudAccounts: DomainCloudAccountsApi;

  constructor(client: HttpClient) {
    this.domainZones = new DomainDomainZonesApi(client);
    this.cloudAccounts = new DomainCloudAccountsApi(client);
  }

}

export function createDomainApi(client: HttpClient): DomainApi {
  return new DomainApi(client);
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
