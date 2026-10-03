/// Wx transport + marketplace port for the deployments mini-program.
///
/// The WeChat runtime has no fetch/XHR, so this is the one surface where a
/// platform transport is written by hand (`wx.request`); every marketplace
/// call still flows through this single typed port, and tests inject a fake
/// requester so the envelope decoding and command shaping stay unit-tested
/// without WeChat DevTools.
///
/// The port spans two app APIs on the same origin: the deployments catalog
/// (`template_categories` / `marketplace/templates` / `app_templates`) and the
/// order center's app-template trade (`app_template_orders`). The deployments
/// module is catalog-only — it owns no purchase, order or entitlement row, so
/// `acquire`/`myPurchases` speak the order center's payloads and nothing here
/// records or settles a payment.

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

/// Order-center order states (`app_template_orders`). `paid` is the settled
/// state; a FREE listing is created settled, a PAID listing starts
/// `pending_payment` until the buyer's payment lands.
export type TemplateOrderStatus = 'paid' | 'pending_payment' | 'closed';

/// How `qrCode` has to be presented: a cashier URL the buyer opens, or a
/// provider-native payment payload.
export type TemplateOrderQrCodeType = 'cashier_url' | 'provider_native';

/// One trade: `POST /app/v3/api/app_template_orders` (`appTemplateOrders.create`).
export interface TemplateOrder {
  readonly orderId: string;
  readonly orderNo: string;
  readonly outTradeNo: string;
  readonly templateUuid: string;
  readonly templateName: string;
  readonly versionUuid?: string;
  readonly amount: string;
  readonly currencyCode: string;
  readonly expiresAt?: string;
  readonly paymentMethod?: string;
  readonly paymentProduct: string;
  readonly qrCode?: string;
  readonly qrCodeType?: TemplateOrderQrCodeType;
  readonly paymentId?: string;
  readonly paymentParams?: Readonly<Record<string, unknown>>;
  readonly status: TemplateOrderStatus;
  /// True when the order center answered with an order this buyer already had
  /// instead of creating another one.
  readonly reused: boolean;
  readonly cashierUrl?: string;
}

/// One buyer order row: `GET /app/v3/api/app_template_orders`
/// (`appTemplateOrders.list`). `status === 'paid'` is the install entitlement.
export interface TemplateOrderSummary {
  readonly orderId: string;
  readonly orderNo: string;
  readonly templateUuid: string;
  readonly templateName: string;
  readonly versionUuid?: string;
  readonly amount: string;
  readonly currencyCode: string;
  readonly status: TemplateOrderStatus;
  readonly fulfillmentStatus: string;
  readonly paidAt?: string;
  readonly createdAt: string;
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

/**
 * What the buyer has to act on: the provider-native payload when the order
 * center marks the QR as such, else the cashier URL. Empty when the order
 * carries no payment requirement at all (a settled order).
 */
export function paymentRequirement(order: TemplateOrder): string {
  const qrCode = order.qrCode ?? '';
  if (order.qrCodeType === 'provider_native' && qrCode !== '') return qrCode;
  return order.cashierUrl ?? qrCode;
}

export class MarketplaceError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'MarketplaceError';
  }
}

// -- port ----------------------------------------------------------------------

export interface MarketplaceBrowseParams {
  page: number;
  pageSize: number;
  keyword?: string;
  categoryUuid?: string;
  templateType?: string;
  pricingModel?: string;
  sort?: string;
}

/** One browse page plus the paging facts the list needs to offer "load more". */
export interface MarketplaceBrowsePage {
  items: TemplateSummary[];
  hasMore: boolean;
  total: number;
}

export interface MarketplacePort {
  categories(): Promise<TemplateCategory[]>;
  browse(params: MarketplaceBrowseParams): Promise<MarketplaceBrowsePage>;
  retrieve(templateUuid: string): Promise<TemplateDetail>;
  /** Order-center trade for a listing; the response carries the payment requirement. */
  acquire(templateUuid: string): Promise<TemplateOrder>;
  /** The buyer's template orders; rows with `status === 'paid'` are entitlements. */
  myPurchases(): Promise<TemplateOrderSummary[]>;
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

  private static pageInfo(payload: unknown): { hasMore?: unknown; totalItems?: unknown } {
    const data = payload as { pageInfo?: unknown } | null;
    const pageInfo = data?.pageInfo;
    return pageInfo && typeof pageInfo === 'object' ? (pageInfo as { hasMore?: unknown; totalItems?: unknown }) : {};
  }

  private static hasMore(payload: unknown): boolean {
    // `hasMore` is the only paging fact the list can trust: a page shorter than
    // pageSize is not proof of the end when the server filters after paging.
    return SdkworkMiniMarketplacePort.pageInfo(payload).hasMore === true;
  }

  private static total(payload: unknown): number {
    const raw = SdkworkMiniMarketplacePort.pageInfo(payload).totalItems;
    const parsed = typeof raw === 'string' ? Number.parseInt(raw, 10) : NaN;
    return Number.isFinite(parsed) ? parsed : 0;
  }

  categories() {
    return this.call<unknown>('GET', '/template_categories?include_disabled=false').then((payload) =>
      SdkworkMiniMarketplacePort.items<TemplateCategory>(payload),
    );
  }

  browse(params: MarketplaceBrowseParams): Promise<MarketplaceBrowsePage> {
    const query = buildQuery([
      ['page', String(params.page)],
      ['page_size', String(params.pageSize)],
      ...(params.keyword && params.keyword.trim() !== '' ? [['keyword', params.keyword.trim()] as const] : []),
      ...(params.categoryUuid ? [['category_uuid', params.categoryUuid] as const] : []),
      ...(params.templateType ? [['template_type', params.templateType] as const] : []),
      ...(params.pricingModel ? [['pricing_model', params.pricingModel] as const] : []),
      ...(params.sort ? [['sort', params.sort] as const] : []),
    ]);
    return this.call<unknown>('GET', `/marketplace/templates${query}`).then((payload) => ({
      items: SdkworkMiniMarketplacePort.items<TemplateSummary>(payload),
      hasMore: SdkworkMiniMarketplacePort.hasMore(payload),
      total: SdkworkMiniMarketplacePort.total(payload),
    }));
  }

  retrieve(templateUuid: string) {
    return this.call<unknown>('GET', `/marketplace/templates/${encodeURIComponent(templateUuid)}`).then((payload) =>
      SdkworkMiniMarketplacePort.item<TemplateDetail>(payload),
    );
  }

  /**
   * Starts the order-center trade with the template uuid in the body (the
   * `Idempotency-Key` header comes from `call`). A FREE listing answers
   * `paid`; a PAID listing answers `pending_payment` plus the cashier URL or
   * provider QR code the buyer still has to settle.
   */
  acquire(templateUuid: string) {
    return this.call<unknown>('POST', '/app_template_orders', { templateUuid }).then((payload) =>
      SdkworkMiniMarketplacePort.item<TemplateOrder>(payload),
    );
  }

  myPurchases() {
    return this.call<unknown>('GET', '/app_template_orders?page=1&page_size=100').then((payload) =>
      SdkworkMiniMarketplacePort.items<TemplateOrderSummary>(payload),
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
