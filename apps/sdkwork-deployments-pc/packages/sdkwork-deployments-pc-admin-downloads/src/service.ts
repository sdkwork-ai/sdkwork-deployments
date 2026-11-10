//! Typed wrapper around the backend download endpoints.
//!
//! The page never builds a URL or a request body shape by hand: every call goes
//! through the generated backend client, so a contract change breaks the build
//! here instead of silently sending a stale field.

import type {
  DownloadCardUpsertRequest,
  DownloadCatalogResponse,
  DownloadCatalogSummaryResponse,
  DownloadCatalogUpsertRequest,
  DownloadItemUpsertRequest,
  DownloadUpdateUpsertRequest,
  SdkworkDeployBackendClient,
} from "@sdkwork/deployments-backend-sdk";

/** Envelope of a successful delete; the API answers 204 with no body. */
export type DownloadsAdminService = ReturnType<typeof createDownloadsAdminService>;

/**
 * Binds the download catalog endpoints to one authenticated backend client.
 * @param client - Backend admin client from `useDeploymentsAdminClient`.
 * @returns The operations the download page renders.
 */
export function createDownloadsAdminService(client: SdkworkDeployBackendClient) {
  return {
    /** Every catalog of the operator's tenant, newest first. */
    list: (): Promise<DownloadCatalogSummaryResponse[]> => client.downloadCatalog.list(),

    /** One catalog with cards, installers and update configurations. */
    retrieve: (catalogKey: string): Promise<DownloadCatalogResponse> =>
      client.downloadCatalog.retrieve(catalogKey),

    /** Creates the catalog when absent and patches the supplied fields otherwise. */
    upsertCatalog: (
      catalogKey: string,
      body: DownloadCatalogUpsertRequest,
    ): Promise<DownloadCatalogResponse> => client.downloadCatalog.upsert(catalogKey, body),

    /** Publishes the catalog so the public download page can read it. */
    publish: (catalogKey: string): Promise<DownloadCatalogResponse> =>
      client.downloadCatalog.publish(catalogKey),

    /** Creates or patches one card. */
    upsertCard: (
      catalogKey: string,
      cardKey: string,
      body: DownloadCardUpsertRequest,
    ) => client.downloadCatalog.downloadCard.upsert(catalogKey, cardKey, body),

    /** Deletes one card and its installers. */
    deleteCard: (catalogKey: string, cardKey: string): Promise<void> =>
      client.downloadCatalog.downloadCard.delete(catalogKey, cardKey),

    /** Creates or patches one installer. */
    upsertItem: (
      catalogKey: string,
      cardKey: string,
      itemKey: string,
      body: DownloadItemUpsertRequest,
    ) => client.downloadCatalog.downloadItem.upsert(catalogKey, cardKey, itemKey, body),

    /** Deletes one installer and its update configuration. */
    deleteItem: (catalogKey: string, cardKey: string, itemKey: string): Promise<void> =>
      client.downloadCatalog.downloadItem.delete(catalogKey, cardKey, itemKey),

    /** Creates or replaces one installer's update configuration. */
    upsertUpdate: (
      catalogKey: string,
      cardKey: string,
      itemKey: string,
      body: DownloadUpdateUpsertRequest,
    ) => client.downloadCatalog.downloadUpdate.upsert(catalogKey, cardKey, itemKey, body),
  };
}
