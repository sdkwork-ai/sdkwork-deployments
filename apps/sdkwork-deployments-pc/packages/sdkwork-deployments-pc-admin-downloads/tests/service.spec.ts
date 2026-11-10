import { describe, expect, it, vi } from 'vitest'

import type { SdkworkDeployBackendClient } from '@sdkwork/deployments-backend-sdk'
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
    downloadCatalog: {
      list: method('list'),
      retrieve: method('retrieve'),
      upsert: method('upsert'),
      publish: method('publish'),
      downloadCard: { upsert: method('card.upsert'), delete: method('card.delete') },
      downloadItem: { upsert: method('item.upsert'), delete: method('item.delete') },
      downloadUpdate: { upsert: method('update.upsert') },
    },
  }
  return { client: client as unknown as SdkworkDeployBackendClient, calls }
}

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
      ['upsert', 'catalog', { productKey: 'app.demo' }],
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
      ['card.upsert', 'catalog', 'desktop', { title: 'Desktop' }],
      ['card.delete', 'catalog', 'desktop'],
      ['item.upsert', 'catalog', 'desktop', 'win-x64', { label: 'Windows' }],
      ['item.delete', 'catalog', 'desktop', 'win-x64'],
      ['update.upsert', 'catalog', 'desktop', 'win-x64', { provider: 'generic' }],
    ])
  })
})
