//! Typed wrapper around the backend download endpoints.
//!
//! The page never builds a URL or a request body shape by hand: every call goes
//! through the generated backend client, so a contract change breaks the build
//! here instead of silently sending a stale field.
//!
//! The catalog surface is published under the client's `downloads` group
//! (`GET/PUT /downloads/catalogs...`); the generated `update` methods carry the
//! required `Idempotency-Key` header parameter, so each write generates one.

import type {
  DownloadCardUpsertRequest,
  DownloadCatalogResponse,
  DownloadCatalogSummaryResponse,
  DownloadCatalogUpsertRequest,
  DownloadItemUpsertRequest,
  DownloadUpdateUpsertRequest,
  SdkworkDeployBackendClient,
} from "@sdkwork/deployments-pc-admin-core";

/** Envelope of a successful delete; the API answers 204 with no body. */
export type DownloadsAdminService = ReturnType<typeof createDownloadsAdminService>;

function idempotencyParams(): { idempotencyKey: string } {
  return { idempotencyKey: crypto.randomUUID() };
}

/**
 * Binds the download catalog endpoints to one authenticated backend client.
 * @param client - Backend admin client from `useDeploymentsAdminClient`.
 * @returns The operations the download page renders.
 */
export function createDownloadsAdminService(client: SdkworkDeployBackendClient) {
  return {
    /** Every catalog of the operator's tenant, newest first. */
    list: (): Promise<DownloadCatalogSummaryResponse[]> =>
      client.downloads.downloadCatalog.list(),

    /** One catalog with cards, installers and update configurations. */
    retrieve: (catalogKey: string): Promise<DownloadCatalogResponse> =>
      client.downloads.downloadCatalog.retrieve(catalogKey),

    /** Creates the catalog when absent and patches the supplied fields otherwise. */
    upsertCatalog: (
      catalogKey: string,
      body: DownloadCatalogUpsertRequest,
    ): Promise<DownloadCatalogResponse> =>
      client.downloads.downloadCatalog.update(catalogKey, body, idempotencyParams()),

    /** Publishes the catalog so the public download page can read it. */
    publish: (catalogKey: string): Promise<DownloadCatalogResponse> =>
      client.downloads.downloadCatalog.publish(catalogKey),

    /** Creates or patches one card. */
    upsertCard: (
      catalogKey: string,
      cardKey: string,
      body: DownloadCardUpsertRequest,
    ) => client.downloads.downloadCard.update(catalogKey, cardKey, body, idempotencyParams()),

    /** Deletes one card and its installers. */
    deleteCard: (catalogKey: string, cardKey: string): Promise<void> =>
      client.downloads.downloadCard.delete(catalogKey, cardKey),

    /** Creates or patches one installer. */
    upsertItem: (
      catalogKey: string,
      cardKey: string,
      itemKey: string,
      body: DownloadItemUpsertRequest,
    ) =>
      client.downloads.downloadItem.update(catalogKey, cardKey, itemKey, body, idempotencyParams()),

    /** Deletes one installer and its update configuration. */
    deleteItem: (catalogKey: string, cardKey: string, itemKey: string): Promise<void> =>
      client.downloads.downloadItem.delete(catalogKey, cardKey, itemKey),

    /** Creates or replaces one installer's update configuration. */
    upsertUpdate: (
      catalogKey: string,
      cardKey: string,
      itemKey: string,
      body: DownloadUpdateUpsertRequest,
    ) =>
      client.downloads.downloadUpdate.update(
        catalogKey,
        cardKey,
        itemKey,
        body,
        idempotencyParams(),
      ),
  };
}
