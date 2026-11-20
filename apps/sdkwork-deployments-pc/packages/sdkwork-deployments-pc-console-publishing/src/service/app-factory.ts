/**
 * Factory operations on one application: derived readiness, forward-only
 * uninstall, remix source state, and the per-environment promotion history.
 *
 * The page never builds a URL or a body shape by hand; every call goes through
 * the generated app client (`@sdkwork/deployments-pc-console-core/sdk`), so a
 * contract change breaks the build here.
 */
import type {
  AppEnvironmentResponse,
  AppFactoryReadiness,
  AppRemixResponse,
  AppUninstallResponse,
  EnvironmentPromotionResponse,
  PageInfo,
  PromoteEnvironmentRequest,
  SdkworkDeployAppClient,
} from "@sdkwork/deployments-pc-console-core/sdk";
import { uuid } from "@sdkwork/utils/id";

/** Everything the factory panel renders for one application. */
export interface AppFactoryState {
  readonly readiness: AppFactoryReadiness;
  readonly remix: AppRemixResponse;
  readonly environments: readonly AppEnvironmentResponse[];
}

/** One page of an environment's immutable promotion history. */
export interface PromotionHistoryPage {
  readonly items: readonly EnvironmentPromotionResponse[];
  readonly pageInfo: PageInfo;
}

/** Input of the promote command; the client supplies the idempotency key. */
export interface PromoteReleaseInput {
  readonly releaseId: string;
  readonly approve?: boolean;
  readonly approverUserId?: string;
  readonly note?: string;
}

export interface AppFactoryService {
  load(appId: string): Promise<AppFactoryState>;
  uninstall(appId: string): Promise<AppUninstallResponse>;
  setRemixEnabled(appId: string, enabled: boolean): Promise<AppRemixResponse>;
  listPromotions(
    appId: string,
    environmentId: string,
    page?: number,
    pageSize?: number,
  ): Promise<PromotionHistoryPage>;
  promote(
    appId: string,
    environmentId: string,
    input: PromoteReleaseInput,
  ): Promise<EnvironmentPromotionResponse>;
}

export interface AppFactoryServiceOptions {
  readonly deployClient: SdkworkDeployAppClient;
  /** Injectable for deterministic tests; defaults to `uuid()`. */
  readonly createIdempotencyKey?: (() => string) | undefined;
}

/**
 * Binds the factory operations to one authenticated app client.
 * @param options - The client and an optional idempotency-key factory.
 */
export function createAppFactoryService(
  options: AppFactoryServiceOptions,
): AppFactoryService {
  const { deployClient } = options;
  const createIdempotencyKey = options.createIdempotencyKey ?? (() => uuid());

  return {
    async load(appId) {
      // Readiness and remix are independent reads: one failure must not hide the
      // other. The environments read decides whether a promotions section can
      // render at all.
      const [readiness, remix, environments] = await Promise.all([
        deployClient.app.readiness.retrieve(appId),
        deployClient.app.remix.retrieve(appId),
        deployClient.appEnvironment.list(appId, { page: 1, pageSize: 50 }),
      ]);
      return { readiness, remix, environments: environments.items };
    },

    async uninstall(appId) {
      return deployClient.app.uninstall(appId, {
        idempotencyKey: createIdempotencyKey(),
      });
    },

    async setRemixEnabled(appId, enabled) {
      return deployClient.app.remix.update(appId, { enabled });
    },

    async listPromotions(appId, environmentId, page = 1, pageSize = 20) {
      return deployClient.appEnvironment.appEnvironmentPromotions.list(
        appId,
        environmentId,
        { page, pageSize },
      );
    },

    async promote(appId, environmentId, input) {
      const body: PromoteEnvironmentRequest = {
        releaseId: input.releaseId,
        ...(input.note === undefined ? {} : { note: input.note }),
        ...(input.approve === undefined ? {} : { approve: input.approve }),
        ...(input.approverUserId === undefined
          ? {}
          : { approverUserId: input.approverUserId }),
      };
      return deployClient.appEnvironment.appEnvironmentPromotions.create(
        appId,
        environmentId,
        body,
        { idempotencyKey: createIdempotencyKey() },
      );
    },
  };
}
