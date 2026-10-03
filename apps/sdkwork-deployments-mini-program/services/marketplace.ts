/// Wx transport + marketplace port for the deployments mini-program.
///
/// The WeChat runtime has no fetch/XHR, so this is the one surface where a
/// platform transport is written by hand (`wx.request`); every marketplace
/// call still flows through this single typed port, and tests inject a fake
/// requester so the envelope decoding and command shaping stay unit-tested
/// without WeChat DevTools.

export interface WxRequestOptions {
  readonly url: string;
  readonly method: 'GET' | 'POST' | 'DELETE';
  readonly data?: unknown;
  readonly header?: Record<string, string>;
}

export interface WxResponseLike {
  readonly statusCode: number;
  readonly data: unknown;
}

export type WxRequester = (options: WxRequestOptions) => Promise<WxResponseLike>;

export function createWxRequester(): WxRequester {
  return (options) =>
    new Promise((resolve, reject) => {
      wx.request({
        url: options.url,
        method: options.method,
        ...(options.data === undefined ? {} : { data: options.data as Record<string, unknown> }),
        ...(options.header === undefined ? {} : { header: options.header }),
        success: (response) =>
          resolve({ statusCode: response.statusCode, data: response.data }),
        fail: (error) => reject(new Error(error.errMsg)),
      });
    });
}

export interface MiniProgramRuntimeOptions {
  readonly appApiBaseUrl: string;
  readonly authToken?: string;
  readonly accessToken?: string;
}

// -- wire models (mirror of the contract responses; int64 ids stay strings) --

export interface TemplateCategory {
  readonly id: string;
  readonly categoryKey: string;
  readonly displayName: string;
  readonly status: string;
}

export interface TemplateSummary {
  readonly id: string;
  readonly templateType: string;
  readonly displayName: string;
  readonly summary: string;
  readonly pricingModel: string;
  readonly priceMinor: string;
  readonly currency: string;
  readonly status: string;
  readonly isFeatured: boolean;
  readonly installCount: string;
}

export interface TemplateDetail {
  readonly id: string;
  readonly displayName: string;
  readonly summary: string;
  readonly description: string;
  readonly pricingModel: string;
  readonly priceMinor: string;
  readonly currency: string;
  readonly status: string;
}

export interface TemplatePurchase {
  readonly id: string;
  readonly templateUuid: string;
  readonly status: string;
  readonly pricingModel: string;
}

export interface MyTemplate {
  readonly id: string;
  readonly displayName: string;
  readonly templateKey: string;
  readonly status: string;
  readonly visibility: string;
}

export interface TemplateVersion {
  readonly id: string;
  readonly templateVersion: string;
  readonly status: string;
  readonly changelog: string;
}

/** Minimal percent-encoding query builder (no URL API in the mini runtime). */
function buildQuery(entries: readonly (readonly [string, string])[]): string {
  const encoded = entries.map(([key, value]) => `${encodeURIComponent(key)}=${encodeURIComponent(value)}`);
  return encoded.length === 0 ? '' : `?${encoded.join('&')}`;
}

export class MarketplaceError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'MarketplaceError';
  }
}

// -- port ----------------------------------------------------------------------

export interface MarketplacePort {
  categories(): Promise<TemplateCategory[]>;
  browse(params: { page: number; pageSize: number; keyword?: string; categoryUuid?: string; templateType?: string }): Promise<TemplateSummary[]>;
  retrieve(templateUuid: string): Promise<TemplateDetail>;
  acquire(templateUuid: string): Promise<TemplatePurchase>;
  myPurchases(): Promise<TemplatePurchase[]>;
  myTemplates(): Promise<MyTemplate[]>;
  versions(templateUuid: string): Promise<TemplateVersion[]>;
  submit(templateUuid: string): Promise<void>;
  withdraw(templateUuid: string): Promise<void>;
}

export class SdkworkMiniMarketplacePort implements MarketplacePort {
  private readonly request: WxRequester;
  private readonly baseUrl: string;
  private readonly tokens: { readonly authToken?: string; readonly accessToken?: string };

  constructor(request: WxRequester, options: MiniProgramRuntimeOptions) {
    this.request = request;
    this.baseUrl = options.appApiBaseUrl.replace(/\/+$/u, '') + '/app/v3/api';
    this.tokens = {
      ...(options.authToken ? { authToken: options.authToken } : {}),
      ...(options.accessToken ? { accessToken: options.accessToken } : {}),
    };
  }

  private async call<T>(method: WxRequestOptions['method'], path: string, body?: unknown): Promise<T> {
    const idempotencyHeader =
      method === 'POST' ? { 'Idempotency-Key': Date.now().toString(36) + Math.random().toString(36).slice(2, 10) } : {};
    const response = await this.request({
      url: this.baseUrl + path,
      method,
      ...(body === undefined ? {} : { data: body }),
      header: {
        ...idempotencyHeader,
        ...(this.tokens.authToken ? { Authorization: `Bearer ${this.tokens.authToken}` } : {}),
        ...(this.tokens.accessToken ? { 'Access-Token': this.tokens.accessToken } : {}),
      },
    });
    if (response.statusCode < 200 || response.statusCode >= 300) {
      throw new MarketplaceError(`request failed with ${String(response.statusCode)}`);
    }
    const envelope = response.data as { code?: number; data?: unknown } | null;
    if (!envelope || typeof envelope !== 'object' || envelope.code !== 0) {
      throw new MarketplaceError('unexpected response envelope');
    }
    return envelope.data as T;
  }

  private static items<T>(payload: unknown): T[] {
    const data = payload as { items?: unknown } | null;
    if (!data || !Array.isArray(data.items)) {
      throw new MarketplaceError('list response carried no items');
    }
    return data.items as T[];
  }

  private static item<T>(payload: unknown): T {
    const data = payload as { item?: unknown } | null;
    if (!data || typeof data.item !== 'object' || data.item === null) {
      throw new MarketplaceError('resource response carried no item');
    }
    return data.item as T;
  }

  categories() {
    return this.call<unknown>('GET', '/template_categories?include_disabled=false').then((payload) =>
      SdkworkMiniMarketplacePort.items<TemplateCategory>(payload),
    );
  }

  browse(params: { page: number; pageSize: number; keyword?: string; categoryUuid?: string; templateType?: string }) {
    const query = buildQuery([
      ['page', String(params.page)],
      ['page_size', String(params.pageSize)],
      ...(params.keyword && params.keyword.trim() !== '' ? [['keyword', params.keyword.trim()] as const] : []),
      ...(params.categoryUuid ? [['category_uuid', params.categoryUuid] as const] : []),
      ...(params.templateType ? [['template_type', params.templateType] as const] : []),
    ]);
    return this.call<unknown>('GET', `/marketplace/templates${query}`).then((payload) =>
      SdkworkMiniMarketplacePort.items<TemplateSummary>(payload),
    );
  }

  retrieve(templateUuid: string) {
    return this.call<unknown>('GET', `/marketplace/templates/${encodeURIComponent(templateUuid)}`).then((payload) =>
      SdkworkMiniMarketplacePort.item<TemplateDetail>(payload),
    );
  }

  acquire(templateUuid: string) {
    return this.call<unknown>('POST', `/marketplace/templates/${encodeURIComponent(templateUuid)}/purchase`, {}).then(
      (payload) => SdkworkMiniMarketplacePort.item<TemplatePurchase>(payload),
    );
  }

  myPurchases() {
    return this.call<unknown>('GET', '/template_purchases?page=1&page_size=100').then((payload) =>
      SdkworkMiniMarketplacePort.items<TemplatePurchase>(payload),
    );
  }

  myTemplates() {
    return this.call<unknown>('GET', '/app_templates?page=1&page_size=50').then((payload) =>
      SdkworkMiniMarketplacePort.items<MyTemplate>(payload),
    );
  }

  versions(templateUuid: string) {
    return this.call<unknown>('GET', `/app_templates/${encodeURIComponent(templateUuid)}/versions?page=1&page_size=50`).then(
      (payload) => SdkworkMiniMarketplacePort.items<TemplateVersion>(payload),
    );
  }

  async submit(templateUuid: string) {
    await this.call<unknown>('POST', `/app_templates/${encodeURIComponent(templateUuid)}/submit`);
  }

  async withdraw(templateUuid: string) {
    await this.call<unknown>('DELETE', `/app_templates/${encodeURIComponent(templateUuid)}`);
  }
}
