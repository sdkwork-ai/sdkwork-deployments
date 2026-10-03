import assert from 'node:assert/strict';
import test from 'node:test';

import {
  MarketplaceError,
  SdkworkMiniMarketplacePort,
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

test('acquire sends the idempotency key header and reads the item envelope', async () => {
  const { port, calls } = fakePort({
    'POST http://127.0.0.1:3900/app/v3/api/marketplace/templates/tmpl-1/purchase': () =>
      envelope({ item: { id: 'pur-1', templateUuid: 'tmpl-1', status: 'ACTIVE', pricingModel: 'FREE' } }),
  });
  const purchase = await port.acquire('tmpl-1');
  assert.equal(purchase.status, 'ACTIVE');
  const first = calls[0];
  if (!first) throw new Error('the acquire call was not recorded');
  const key = first.header?.['Idempotency-Key'];
  assert.ok(typeof key === 'string' && key.length > 8);
});

test('non-2xx and broken envelopes fail closed', async () => {
  const { port } = fakePort({
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
