import assert from 'node:assert/strict';
import test from 'node:test';

import {
  MarketplaceError,
  SdkworkMiniMarketplacePort,
  paymentRequirement,
  type TemplateOrder,
  type WxRequestOptions,
  type WxResponseLike,
} from '../services/marketplace.ts';

type Responder = (options: WxRequestOptions) => WxResponseLike;

function envelope(data: unknown): WxResponseLike {
  return { statusCode: 200, data: { code: 0, data, traceId: 't-1' } };
}

const cannedSummary = {
  id: 'tmpl-1',
  displayName: 'Blog starter',
  summary: 'A blog',
  pricingModel: 'FREE',
  priceMinor: '0',
  currency: 'CNY',
  status: 'PUBLISHED',
  isFeatured: false,
  installCount: '3',
};

/** Fake requester recording calls and answering canned envelopes. */
function fakePort(routes: Record<string, Responder>): {
  port: SdkworkMiniMarketplacePort;
  calls: WxRequestOptions[];
} {
  const calls: WxRequestOptions[] = [];
  const request = async (options: WxRequestOptions): Promise<WxResponseLike> => {
    calls.push(options);
    // Route lookup ignores the query string so canned keys stay readable.
    const responder = routes[`${options.method} ${options.url.split('?')[0] ?? ''}`];
    if (responder === undefined) {
      throw new Error('no canned route for ' + options.url);
    }
    return responder(options);
  };
  return {
    port: new SdkworkMiniMarketplacePort(request, { appApiBaseUrl: 'http://127.0.0.1:3900/' }),
    calls,
  };
}

test('browse projects blank facets and hits the marketplace path with snake_case query', async () => {
  const { port, calls } = fakePort({
    'GET http://127.0.0.1:3900/app/v3/api/marketplace/templates': () =>
      envelope({ items: [cannedSummary], pageInfo: { hasMore: true, totalItems: '7' } }),
  });
  const page = await port.browse({ page: 1, pageSize: 10, keyword: '   ', categoryUuid: '' });
  assert.equal(page.items.length, 1);
  assert.equal(page.items[0]?.displayName, 'Blog starter');
  assert.equal(page.hasMore, true);
  assert.equal(page.total, 7);
  const first = calls[0];
  if (!first) throw new Error('the browse call was not recorded');
  const url = new URL(first.url);
  assert.equal(url.searchParams.get('page'), '1');
  assert.equal(url.searchParams.get('page_size'), '10');
  assert.equal(url.searchParams.has('keyword'), false);
  assert.equal(url.searchParams.has('category_uuid'), false);
  assert.equal(url.searchParams.has('pricing_model'), false);
  assert.equal(url.searchParams.has('sort'), false);
});

test('browse carries the pricing and sort facets and defaults missing paging facts', async () => {
  const { port, calls } = fakePort({
    'GET http://127.0.0.1:3900/app/v3/api/marketplace/templates': () =>
      envelope({ items: [cannedSummary], pageInfo: {} }),
  });
  const page = await port.browse({ page: 2, pageSize: 10, pricingModel: 'PAID', sort: 'POPULAR' });
  assert.equal(page.hasMore, false);
  assert.equal(page.total, 0);
  const first = calls[0];
  if (!first) throw new Error('the browse call was not recorded');
  const url = new URL(first.url);
  assert.equal(url.searchParams.get('pricing_model'), 'PAID');
  assert.equal(url.searchParams.get('sort'), 'POPULAR');
  assert.equal(url.searchParams.get('page'), '2');
});

test('acquire posts the order-center command with the uuid in the body and an idempotency key', async () => {
  const { port, calls } = fakePort({
    'POST http://127.0.0.1:3900/app/v3/api/app_template_orders': () =>
      envelope({
        item: {
          orderId: 'ord-1',
          orderNo: 'T20261003000001',
          outTradeNo: 'ot-1',
          templateUuid: 'tmpl-1',
          templateName: 'Blog starter',
          amount: '1990',
          currencyCode: 'CNY',
          expiresAt: '2026-10-03T01:00:00Z',
          paymentMethod: 'wechat',
          paymentProduct: 'mobile_cashier_h5',
          qrCode: 'https://cashier.example/ord-1',
          qrCodeType: 'cashier_url',
          status: 'pending_payment',
          reused: false,
          cashierUrl: 'https://cashier.example/ord-1',
        },
      }),
  });
  const order = await port.acquire('tmpl-1');
  // A PAID listing is not refused client-side: the trade starts and the
  // response carries what the buyer still has to pay.
  assert.equal(order.status, 'pending_payment');
  assert.equal(order.cashierUrl, 'https://cashier.example/ord-1');
  assert.equal(order.qrCodeType, 'cashier_url');
  const first = calls[0];
  if (!first) throw new Error('the acquire call was not recorded');
  assert.equal(first.method, 'POST');
  assert.equal(first.url, 'http://127.0.0.1:3900/app/v3/api/app_template_orders');
  assert.deepEqual(first.data, { templateUuid: 'tmpl-1' });
  const key = first.header?.['Idempotency-Key'];
  assert.ok(typeof key === 'string' && key.length > 8);
});

test('a FREE acquire reads the settled order out of the item envelope', async () => {
  const { port } = fakePort({
    'POST http://127.0.0.1:3900/app/v3/api/app_template_orders': () =>
      envelope({
        item: {
          orderId: 'ord-2',
          orderNo: 'T20261003000002',
          outTradeNo: 'ot-2',
          templateUuid: 'tmpl-1',
          templateName: 'Blog starter',
          amount: '0',
          currencyCode: 'CNY',
          paymentProduct: 'mobile_cashier_h5',
          status: 'paid',
          reused: false,
        },
      }),
  });
  const order = await port.acquire('tmpl-1');
  assert.equal(order.status, 'paid');
  assert.equal(order.cashierUrl, undefined);
});

test('myPurchases reads the order-center page and keeps only paid rows as entitlements', async () => {
  const { port, calls } = fakePort({
    'GET http://127.0.0.1:3900/app/v3/api/app_template_orders': () =>
      envelope({
        items: [
          {
            orderId: 'ord-1',
            orderNo: 'T20261003000001',
            templateUuid: 'tmpl-1',
            templateName: 'Blog starter',
            versionUuid: 'ver-1',
            amount: '0',
            currencyCode: 'CNY',
            status: 'paid',
            fulfillmentStatus: 'fulfilled',
            paidAt: '2026-10-03T00:10:00Z',
            createdAt: '2026-10-03T00:10:00Z',
          },
          {
            orderId: 'ord-2',
            orderNo: 'T20261003000002',
            templateUuid: 'tmpl-2',
            templateName: 'Deck starter',
            amount: '1990',
            currencyCode: 'CNY',
            status: 'pending_payment',
            fulfillmentStatus: 'unfulfilled',
            createdAt: '2026-10-03T00:20:00Z',
          },
        ],
        pageInfo: { hasMore: false, totalItems: '2' },
      }),
  });
  const orders = await port.myPurchases();
  assert.equal(orders.length, 2);
  assert.deepEqual(
    orders.filter((order) => order.status === 'paid').map((order) => order.templateUuid),
    ['tmpl-1'],
  );
  assert.equal(orders[1]?.fulfillmentStatus, 'unfulfilled');
  const first = calls[0];
  if (!first) throw new Error('the myPurchases call was not recorded');
  const url = new URL(first.url);
  assert.equal(url.pathname, '/app/v3/api/app_template_orders');
  assert.equal(url.searchParams.get('page'), '1');
  assert.equal(url.searchParams.get('page_size'), '100');
});

test('paymentRequirement prefers the provider payload only when the order says so', () => {
  const base: TemplateOrder = {
    orderId: 'ord-1',
    orderNo: 'T20261003000001',
    outTradeNo: 'ot-1',
    templateUuid: 'tmpl-1',
    templateName: 'Blog starter',
    amount: '1990',
    currencyCode: 'CNY',
    paymentProduct: 'mobile_cashier_h5',
    status: 'pending_payment',
    reused: false,
  };
  assert.equal(
    paymentRequirement({ ...base, cashierUrl: 'https://cashier.example/ord-1', qrCodeType: 'cashier_url', qrCode: 'https://cashier.example/ord-1' }),
    'https://cashier.example/ord-1',
  );
  assert.equal(
    paymentRequirement({ ...base, qrCodeType: 'provider_native', qrCode: 'weixin://wxpay/bizpayurl?pr=abc', cashierUrl: 'https://cashier.example/ord-1' }),
    'weixin://wxpay/bizpayurl?pr=abc',
  );
  // A settled order carries no requirement at all.
  assert.equal(paymentRequirement({ ...base, status: 'paid' }), '');
});

test('non-2xx and broken envelopes fail closed', async () => {  const { port } = fakePort({
    'GET http://127.0.0.1:3900/app/v3/api/marketplace/templates': () => ({ statusCode: 500, data: null }),
    'GET http://127.0.0.1:3900/app/v3/api/template_categories': () => ({
      statusCode: 200,
      data: { code: 7, data: null },
    }),
  });
  await assert.rejects(port.browse({ page: 1, pageSize: 10 }), MarketplaceError);
  await assert.rejects(port.categories(), MarketplaceError);
});

test('tokens are attached per request', async () => {
  const calls: WxRequestOptions[] = [];
  const request = async (options: WxRequestOptions): Promise<WxResponseLike> => {
    calls.push(options);
    return envelope({ items: [], pageInfo: {} });
  };
  const port = new SdkworkMiniMarketplacePort(request, {
    appApiBaseUrl: 'http://127.0.0.1:3900',
    authToken: 'tok-1',
    accessToken: 'acc-1',
  });
  await port.browse({ page: 1, pageSize: 10 });
  const first = calls[0];
  if (!first) throw new Error('the browse call was not recorded');
  assert.equal(first.header?.Authorization, 'Bearer tok-1');
  assert.equal(first.header?.['Access-Token'], 'acc-1');
});
