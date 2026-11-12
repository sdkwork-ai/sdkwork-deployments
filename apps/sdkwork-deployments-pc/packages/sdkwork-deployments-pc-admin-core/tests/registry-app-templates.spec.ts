import type { SdkworkDeployAppClient } from "@sdkwork/deployments-app-sdk";
import type { SdkworkDeployBackendClient } from "@sdkwork/deployments-backend-sdk";
import { describe, expect, it } from "vitest";

import { createDeploymentsAdminRegistry } from "../src/index.tsx";

/**
 * The application center's template actions are plane-dependent.
 *
 * Listing create/edit exist only on the app API, so those two actions appear
 * only when the host injects the app client. Hosts that bridge the catalog with
 * the backend client alone (the Web Server admin surface) must keep the
 * moderation-only action set instead of failing to compose the registry.
 */
describe("app template actions", () => {
  const backend = {} as unknown as SdkworkDeployBackendClient;

  it("offers moderation only when no app client is injected", () => {
    const actions = createDeploymentsAdminRegistry(backend).appTemplates?.actions ?? [];
    const ids = actions.map((action) => action.id);
    expect(ids).not.toContain("create");
    expect(ids).not.toContain("update");
    expect(ids).toEqual(expect.arrayContaining(["publish", "reject", "disable", "delete"]));
  });

  it("adds listing authoring when the app client is injected", () => {
    const app = {} as unknown as SdkworkDeployAppClient;
    const ids = (createDeploymentsAdminRegistry(backend, app).appTemplates?.actions ?? []).map(
      (action) => action.id,
    );
    expect(ids).toEqual(expect.arrayContaining(["create", "update", "publish", "delete"]));
  });
});
