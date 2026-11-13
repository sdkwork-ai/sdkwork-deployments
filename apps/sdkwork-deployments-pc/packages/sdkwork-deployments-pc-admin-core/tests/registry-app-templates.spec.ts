import type {
  CreateAppTemplateRequest,
  SdkworkDeployAppClient,
  UpdateAppTemplateRequest,
} from "@sdkwork/deployments-app-sdk";
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

  it("sends the template presentation media and validates the screenshot gallery", async () => {
    const captured: {
      create?: CreateAppTemplateRequest;
      update?: UpdateAppTemplateRequest;
    } = {};
    const app = {
      template: {
        appTemplates: {
          create: async (body: CreateAppTemplateRequest) => {
            captured.create = body;
            return {};
          },
          update: async (_templateUuid: string, body: UpdateAppTemplateRequest) => {
            captured.update = body;
            return {};
          },
        },
      },
    } as unknown as SdkworkDeployAppClient;
    const actions = createDeploymentsAdminRegistry(backend, app).appTemplates?.actions ?? [];
    const create = actions.find((action) => action.id === "create");
    const update = actions.find((action) => action.id === "update");
    if (!create || !update) {
      throw new Error("listing authoring actions are missing");
    }

    const base = {
      appUuid: "app-1",
      categoryUuid: "category-1",
      templateKey: "tpl-1",
      displayName: "Demo template",
      summary: "One line",
    };
    await create.execute({
      body: {
        ...base,
        iconMediaRef: "drive://icon-1",
        coverMediaRef: "drive://cover-1",
        screenshots: '[{"mediaRef":"drive://shot-1","device":"PC","caption":"Home","sortOrder":0}]',
      },
    });
    expect(captured.create).toMatchObject({
      appUuid: "app-1",
      iconMediaRef: "drive://icon-1",
      coverMediaRef: "drive://cover-1",
      screenshots: [
        { mediaRef: "drive://shot-1", device: "PC", caption: "Home", sortOrder: 0 },
      ],
    });

    await update.execute({
      body: { coverMediaRef: "drive://cover-2", screenshots: "" },
      selectedItem: { id: "template-1" },
    });
    expect(captured.update).toEqual({ coverMediaRef: "drive://cover-2" });

    await expect(
      create.execute({ body: { ...base, screenshots: "not-json" } }),
    ).rejects.toThrow("screenshots must be a JSON array");
  });
});
