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
export type DeploymentsAppTemplateType = "APP" | "PPT" | "VIDEO";

/** Listing visibility accepted by the author command. */
export type DeploymentsAppTemplateVisibility = "PUBLIC" | "PRIVATE";

/** Listing pricing model accepted by the author command. */
export type DeploymentsAppTemplatePricingModel = "FREE" | "PAID";

/** One screenshot entry of the listing gallery. */
export interface DeploymentsAppTemplateScreenshot {
  mediaRef: string;
  device?: string;
  caption?: string;
  sortOrder?: number;
}

/** Payload of the app-plane `template.appTemplates.create` command. */
export interface DeploymentsAppTemplateCreateBody {
  appUuid: string;
  categoryUuid: string;
  templateKey: string;
  displayName: string;
  summary: string;
  templateType?: DeploymentsAppTemplateType;
  visibility?: DeploymentsAppTemplateVisibility;
  pricingModel?: DeploymentsAppTemplatePricingModel;
  description?: string;
  priceMinor?: string;
  currency?: string;
  iconMediaRef?: string;
  coverMediaRef?: string;
  screenshots?: DeploymentsAppTemplateScreenshot[];
}

/** Payload of the app-plane `template.appTemplates.update` editable-copy patch. */
export interface DeploymentsAppTemplateUpdateBody {
  displayName?: string;
  summary?: string;
  description?: string;
  categoryUuid?: string;
  visibility?: DeploymentsAppTemplateVisibility;
  pricingModel?: DeploymentsAppTemplatePricingModel;
  priceMinor?: string;
  currency?: string;
  iconMediaRef?: string;
  coverMediaRef?: string;
  screenshots?: DeploymentsAppTemplateScreenshot[];
}

/** Idempotency parameters shared by the author commands. */
export interface DeploymentsAppIdempotencyParams {
  idempotencyKey: string;
}

/** The app-plane template client surface the admin registry consumes. */
export interface DeploymentsAppPlaneClient {
  template: {
    appTemplates: {
      create(
        body: DeploymentsAppTemplateCreateBody,
        params: DeploymentsAppIdempotencyParams,
      ): Promise<unknown>;
      update(
        templateId: string,
        body: DeploymentsAppTemplateUpdateBody,
        params: DeploymentsAppIdempotencyParams,
      ): Promise<unknown>;
    };
  };
}
