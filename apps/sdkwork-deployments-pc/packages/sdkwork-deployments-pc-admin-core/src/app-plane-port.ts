/**
 * App-plane port owned by the backend-admin core.
 *
 * `sdkwork-deployments-pc-admin-core` is the backend-admin composition root; the
 * generated app-api SDK must not appear in it (`verify-repo` /
 * `check-application-layering.mjs`). App-template authoring exists only on the
 * app API, so the shell injects an app-plane client that satisfies this
 * structural port, and core keeps only the shapes it assembles.
 */

/** Listing template type accepted by the author command. */
export type DeploymentsApplicationScaffoldType = "APP" | "PPT" | "VIDEO";

/** Listing visibility accepted by the author command. */
export type DeploymentsApplicationScaffoldVisibility = "PUBLIC" | "PRIVATE";

/** Listing pricing model accepted by the author command. */
export type DeploymentsApplicationScaffoldPricingModel = "FREE" | "PAID";

/** One screenshot entry of the listing gallery. */
export interface DeploymentsApplicationScaffoldScreenshot {
  mediaRef: string;
  device?: string;
  caption?: string;
  sortOrder?: number;
}

/** Payload of the app-plane `app.scaffolds.create` command. */
export interface DeploymentsApplicationScaffoldCreateBody {
  appUuid: string;
  categoryUuid: string;
  templateKey: string;
  displayName: string;
  summary: string;
  templateType?: DeploymentsApplicationScaffoldType;
  visibility?: DeploymentsApplicationScaffoldVisibility;
  pricingModel?: DeploymentsApplicationScaffoldPricingModel;
  description?: string;
  priceMinor?: string;
  currency?: string;
  iconMediaRef?: string;
  coverMediaRef?: string;
  screenshots?: DeploymentsApplicationScaffoldScreenshot[];
}

/** Payload of the app-plane `app.scaffolds.update` editable-copy patch. */
export interface DeploymentsApplicationScaffoldUpdateBody {
  displayName?: string;
  summary?: string;
  description?: string;
  categoryUuid?: string;
  visibility?: DeploymentsApplicationScaffoldVisibility;
  pricingModel?: DeploymentsApplicationScaffoldPricingModel;
  priceMinor?: string;
  currency?: string;
  iconMediaRef?: string;
  coverMediaRef?: string;
  screenshots?: DeploymentsApplicationScaffoldScreenshot[];
}

/** Idempotency parameters shared by the author commands. */
export interface DeploymentsAppIdempotencyParams {
  idempotencyKey: string;
}

/** The app-plane application-scaffold client surface the admin registry consumes. */
export interface DeploymentsAppPlaneClient {
  app: {
    scaffolds: {
      create(
        body: DeploymentsApplicationScaffoldCreateBody,
        params: DeploymentsAppIdempotencyParams,
      ): Promise<unknown>;
      update(
        scaffoldId: string,
        body: DeploymentsApplicationScaffoldUpdateBody,
        params: DeploymentsAppIdempotencyParams,
      ): Promise<unknown>;
    };
  };
}
