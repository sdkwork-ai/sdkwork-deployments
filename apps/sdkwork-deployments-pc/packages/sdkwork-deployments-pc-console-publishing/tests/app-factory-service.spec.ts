import { describe, expect, it, vi } from "vitest";

import type { SdkworkDeployAppClient } from "@sdkwork/deployments-pc-console-core/sdk";
import { createAppFactoryService } from "../src/service/app-factory.ts";

function fakeClient() {
  const readiness = { retrieve: vi.fn().mockResolvedValue({ status: "READY", checks: [] }) };
  const remix = {
    retrieve: vi.fn().mockResolvedValue({ appId: "app-1", remixEnabled: false, readiness: "MISSING" }),
    update: vi.fn().mockResolvedValue({ appId: "app-1", remixEnabled: true, readiness: "MISSING" }),
  };
  const uninstall = vi.fn().mockResolvedValue({
    appId: "app-1",
    uninstallStatus: "complete",
    appStatus: "ARCHIVED",
    replayed: false,
  });
  const list = vi.fn().mockResolvedValue({ items: [{ id: "env-1" }], pageInfo: { page: 1, pageSize: 50 } });
  const promotions = {
    list: vi.fn().mockResolvedValue({ items: [], pageInfo: { page: 1, pageSize: 20 } }),
    create: vi.fn().mockResolvedValue({ id: "promo-1" }),
  };
  const client = {
    app: { readiness, remix, uninstall },
    appEnvironment: { list, appEnvironmentPromotions: promotions },
  };
  return { client: client as unknown as SdkworkDeployAppClient, readiness, remix, uninstall, list, promotions };
}

describe("app factory service", () => {
  it("loads readiness, remix and environments together", async () => {
    const { client, readiness, remix, list } = fakeClient();
    const service = createAppFactoryService({ deployClient: client, createIdempotencyKey: () => "idem-1" });

    const state = await service.load("app-1");

    expect(state.readiness.status).toBe("READY");
    expect(state.remix.readiness).toBe("MISSING");
    expect(state.environments).toHaveLength(1);
    expect(readiness.retrieve).toHaveBeenCalledWith("app-1");
    expect(remix.retrieve).toHaveBeenCalledWith("app-1");
    expect(list).toHaveBeenCalledWith("app-1", { page: 1, pageSize: 50 });
  });

  it("sends the command idempotency key and omits absent approval fields", async () => {
    const { client, uninstall, promotions } = fakeClient();
    const service = createAppFactoryService({ deployClient: client, createIdempotencyKey: () => "idem-1" });

    await service.uninstall("app-1");
    await service.promote("app-1", "env-1", { releaseId: "rel-1" });

    expect(uninstall).toHaveBeenCalledWith("app-1", { idempotencyKey: "idem-1" });
    expect(promotions.create).toHaveBeenCalledWith("app-1", "env-1", { releaseId: "rel-1" }, { idempotencyKey: "idem-1" });
  });

  it("carries approval fields only when supplied", async () => {
    const { client, promotions } = fakeClient();
    const service = createAppFactoryService({ deployClient: client, createIdempotencyKey: () => "idem-2" });

    await service.promote("app-1", "env-1", {
      releaseId: "rel-1",
      approve: true,
      approverUserId: "12",
      note: "approved by release manager",
    });

    expect(promotions.create).toHaveBeenCalledWith(
      "app-1",
      "env-1",
      {
        releaseId: "rel-1",
        note: "approved by release manager",
        approve: true,
        approverUserId: "12",
      },
      { idempotencyKey: "idem-2" },
    );
  });

  it("toggles remix through the documented request body", async () => {
    const { client, remix } = fakeClient();
    const service = createAppFactoryService({ deployClient: client, createIdempotencyKey: () => "idem-3" });

    await service.setRemixEnabled("app-1", true);

    expect(remix.update).toHaveBeenCalledWith("app-1", { enabled: true });
  });
});
