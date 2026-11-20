import { describe, expect, it, vi } from 'vitest'

import type { SdkworkDeployBackendClient } from '@sdkwork/deployments-pc-admin-core'
import { createDownloadsAdminService } from '../src/service.ts'

type Call = readonly [name: string, ...args: readonly unknown[]]

function fakeClient(): { client: SdkworkDeployBackendClient; calls: Call[] } {
  const calls: Call[] = []
  const method = (name: string) =>
    vi.fn(async (...args: unknown[]) => {
      calls.push([name, ...args])
      return {}
    })
  const client = {
    downloads: {
      downloadCatalog: {
        list: method('list'),
        retrieve: method('retrieve'),
        update: method('update'),
        publish: method('publish'),
      },
      downloadCard: { update: method('card.update'), delete: method('card.delete') },
      downloadItem: { update: method('item.update'), delete: method('item.delete') },
      downloadUpdate: { update: method('update.update') },
    },
  }
  return { client: client as unknown as SdkworkDeployBackendClient, calls }
}

const anyIdempotency = { idempotencyKey: expect.any(String) }

describe('downloads admin service', () => {
  it('delegates every catalog operation to the generated client', async () => {
    const { client, calls } = fakeClient()
    const service = createDownloadsAdminService(client)

    await service.list()
    await service.retrieve('catalog')
    await service.upsertCatalog('catalog', { productKey: 'app.demo' })
    await service.publish('catalog')

    expect(calls).toEqual([
      ['list'],
      ['retrieve', 'catalog'],
      ['update', 'catalog', { productKey: 'app.demo' }, anyIdempotency],
      ['publish', 'catalog'],
    ])
  })

  it('passes the catalog/card/item path keys through unchanged', async () => {
    const { client, calls } = fakeClient()
    const service = createDownloadsAdminService(client)

    await service.upsertCard('catalog', 'desktop', { title: 'Desktop' })
    await service.deleteCard('catalog', 'desktop')
    await service.upsertItem('catalog', 'desktop', 'win-x64', { label: 'Windows' })
    await service.deleteItem('catalog', 'desktop', 'win-x64')
    await service.upsertUpdate('catalog', 'desktop', 'win-x64', { provider: 'generic' })

    expect(calls).toEqual([
      ['card.update', 'catalog', 'desktop', { title: 'Desktop' }, anyIdempotency],
      ['card.delete', 'catalog', 'desktop'],
      ['item.update', 'catalog', 'desktop', 'win-x64', { label: 'Windows' }, anyIdempotency],
      ['item.delete', 'catalog', 'desktop', 'win-x64'],
      [
        'update.update',
        'catalog',
        'desktop',
        'win-x64',
        { provider: 'generic' },
        anyIdempotency,
      ],
    ])
  })
})
