import { useCallback, useEffect, useMemo, useState } from "react";

import type { AppTemplateResponse, AppTemplateVersionResponse } from "@sdkwork/deployments-app-sdk";

import { translateH5, type DeploymentsH5MessageKey } from "./i18n.ts";
import { createH5MarketplaceService } from "./service.ts";
import type { DeploymentsH5Runtime } from "../bootstrap/runtime.ts";

const PAGE_SIZE = 10;

function translator(locale: string) {
  return (key: DeploymentsH5MessageKey, values?: Record<string, string | number>) => translateH5(locale, key, values);
}

/**
 * Author workbench, H5 shape: the caller's listings with review state, plus
 * per-listing version history and the submit/withdraw commands. Publishing a
 * NEW listing stays on the PC console for now — the H5 surface manages
 * existing listings, which keeps the first mobile cut honest about what it
 * can edit (version creation from a packaged artifact is a PC/Drive flow).
 */
export function MyTemplatesView({ runtime }: { runtime: DeploymentsH5Runtime }) {
  const t = translator(runtime.locale);
  const service = useMemo(() => createH5MarketplaceService(runtime.deploy, runtime.order), [runtime.deploy, runtime.order]);
  const [items, setItems] = useState<readonly AppTemplateResponse[]>([]);
  const [expanded, setExpanded] = useState<string>();
  const [versions, setVersions] = useState<readonly AppTemplateVersionResponse[]>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>();
  const [actionError, setActionError] = useState<string>();

  const load = useCallback(async (): Promise<void> => {
    setBusy(true);
    setError(undefined);
    try {
      const listings = await service.myTemplates(1, PAGE_SIZE);
      setItems(listings.items);
    } catch {
      setError(t("myTemplates.loadFailed"));
    } finally {
      setBusy(false);
    }
  }, [service, t]);

  useEffect(() => {
    void load();
  }, [load]);

  const openVersions = useCallback(
    async (templateUuid: string): Promise<void> => {
      setActionError(undefined);
      if (expanded === templateUuid) {
        setExpanded(undefined);
        return;
      }
      setExpanded(templateUuid);
      try {
        setVersions(await service.listVersions(templateUuid));
      } catch {
        setActionError(t("myTemplates.loadFailed"));
      }
    },
    [expanded, service, t],
  );

  const runAction = useCallback(
    async (action: (templateUuid: string) => Promise<unknown>, templateUuid: string): Promise<void> => {
      setActionError(undefined);
      try {
        await action(templateUuid);
        await load();
      } catch {
        setActionError(t("myTemplates.operationFailed"));
      }
    },
    [load, t],
  );

  return (
    <section className="h5-view">
      <h1 className="h5-view-title">{t("myTemplates.title")}</h1>
      {error && (
        <div className="h5-error" role="alert">
          {error}
          <button type="button" onClick={() => void load()}>
            {t("common.retry")}
          </button>
        </div>
      )}
      {actionError && <div className="h5-error" role="alert">{actionError}</div>}
      <div className="h5-cards">
        {items.map((item) => (
          <div className="h5-card h5-card-static" key={item.id}>
            <button className="h5-card-title h5-card-open" type="button" onClick={() => void openVersions(item.id)}>
              {item.displayName}
              <span className="h5-card-key">{item.templateKey}</span>
            </button>
            <span className="h5-card-summary">{item.summary}</span>
            <span className="h5-card-meta">
              <span className={`h5-status h5-status-${item.status.toLowerCase()}`}>{item.status}</span>
              <span>{item.visibility}</span>
            </span>
            {expanded === item.id && (
              <div className="h5-versions">
                <h3>{t("myTemplates.versions")}</h3>
                {versions.length === 0 ? (
                  <div className="h5-empty">{t("myTemplates.versions.empty")}</div>
                ) : (
                  <ul className="h5-list">
                    {versions.map((version) => (
                      <li key={version.id}>
                        <span className={`h5-status h5-status-${version.status.toLowerCase()}`}>{version.status}</span>
                        <span className="h5-list-main">{version.templateVersion}</span>
                      </li>
                    ))}
                  </ul>
                )}
                <div className="h5-actions">
                  <button
                    className="h5-primary"
                    disabled={busy}
                    type="button"
                    onClick={() => void runAction((templateUuid) => service.submit(templateUuid), item.id)}
                  >
                    {t("myTemplates.submit")}
                  </button>
                  <button
                    className="h5-danger"
                    disabled={busy}
                    type="button"
                    onClick={() => {
                      setExpanded(undefined);
                      void runAction((templateUuid) => service.remove(templateUuid), item.id);
                    }}
                  >
                    {t("myTemplates.withdraw")}
                  </button>
                </div>
              </div>
            )}
          </div>
        ))}
        {items.length === 0 && !busy && <div className="h5-empty">{t("myTemplates.empty")}</div>}
      </div>
    </section>
  );
}
