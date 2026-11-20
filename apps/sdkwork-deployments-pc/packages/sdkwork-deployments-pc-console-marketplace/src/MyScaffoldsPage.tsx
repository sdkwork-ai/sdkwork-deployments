import { Plus, RefreshCw, X } from "lucide-react";
import { useCallback, useEffect, useMemo, useState, type FormEvent } from "react";

import { DataTable, type DataTableColumn } from "@sdkwork/ui-pc-react";

import type {
  ApplicationScaffoldResponse,
  ApplicationScaffoldVersionResponse,
  SdkworkDeployAppClient,
  ScaffoldCategoryResponse,
} from "@sdkwork/deployments-pc-console-core/sdk";
import type { DeploymentsLocale } from "@sdkwork/deployments-pc-commons";

import { marketplaceTranslator } from "./i18n.ts";
import { createMyScaffoldsService, type ScaffoldType } from "./service/marketplace.ts";

export interface MyScaffoldsPageProps {
  readonly deployClient: SdkworkDeployAppClient;
  readonly locale: DeploymentsLocale;
}

const PAGE_SIZE = 20;

/**
 * Author workbench: publish one of the caller's apps as a scaffold listing,
 * track the review state, add versions, and submit for moderation. Every
 * write is a marketplace command (`create` idempotent, `submit`/version
 * transitions server-validated), so failures surface as the honest error
 * banner instead of optimistic row edits.
 */
export function MyScaffoldsPage({ deployClient, locale }: MyScaffoldsPageProps) {
  // The translator must be reference-stable per locale: `load` names `t` in its
  // dependency list, and a fresh closure every render would re-create `load`
  // every render, re-fire the load effect, and refetch the page in a loop.
  const t = useMemo(() => marketplaceTranslator(locale), [locale]);
  const service = useMemo(() => createMyScaffoldsService(deployClient), [deployClient]);
  const [items, setItems] = useState<readonly ApplicationScaffoldResponse[]>([]);
  const [total, setTotal] = useState(0);
  const [hasMore, setHasMore] = useState(false);
  const [page, setPage] = useState(1);
  const [apps, setApps] = useState<readonly { id: string; name: string }[]>([]);
  const [categories, setCategories] = useState<readonly ScaffoldCategoryResponse[]>([]);
  const [expanded, setExpanded] = useState<string>();
  const [versions, setVersions] = useState<readonly ApplicationScaffoldVersionResponse[]>([]);
  const [editTarget, setEditTarget] = useState<ApplicationScaffoldResponse>();
  const [dialog, setDialog] = useState<"create" | "version">();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>();
  const [actionError, setActionError] = useState<string>();

  const load = useCallback(async (): Promise<void> => {
    setBusy(true);
    setError(undefined);
    try {
      const listings = await service.list(page, PAGE_SIZE);
      setItems(listings.items);
      setTotal(listings.total);
      setHasMore(listings.hasMore);
    } catch {
      setError(t("myScaffolds.loadFailed"));
    } finally {
      setBusy(false);
    }
  }, [service, page, t]);

  useEffect(() => {
    void load();
  }, [load]);

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        const [appList, categoryList] = await Promise.all([service.listApps(), deployClient.app.scaffolds.categories.list({ includeDisabled: false })]);
        if (cancelled) return;
        setApps(appList);
        setCategories(categoryList.items);
      } catch {
        // The pickers stay empty; the create dialog is unusable but the ledger
        // still renders. The load failure banner covers transport errors.
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [service, deployClient]);

  const openVersions = useCallback(
    async (templateUuid: string): Promise<void> => {
      setActionError(undefined);
      setExpanded(templateUuid);
      try {
        setVersions(await service.listVersions(templateUuid));
      } catch {
        setActionError(t("myScaffolds.loadFailed"));
      }
    },
    [service, t],
  );

  const openEdit = useCallback(
    async (templateUuid: string): Promise<void> => {
      setActionError(undefined);
      try {
        // Read the listing back before editing: the row in the ledger may be a
        // page-old snapshot, and the dialog must show the stored values it is
        // about to patch.
        setEditTarget(await service.retrieve(templateUuid));
      } catch {
        setActionError(t("myScaffolds.loadFailed"));
      }
    },
    [service, t],
  );

  const runTemplateAction = useCallback(
    async (action: (templateUuid: string) => Promise<unknown>, templateUuid: string): Promise<void> => {
      setActionError(undefined);
      try {
        await action(templateUuid);
        await load();
      } catch {
        setActionError(t("myScaffolds.operationFailed"));
      }
    },
    [load, t],
  );

  const columns = useMemo<DataTableColumn<ApplicationScaffoldResponse>[]>(
    () => [
      {
        id: "displayName",
        header: t("myScaffolds.create.displayName"),
        cell: (item) => (
          <button className="link-button" type="button" onClick={() => void openVersions(item.id)}>
            {item.displayName}
          </button>
        ),
      },
      { id: "templateKey", header: t("myScaffolds.create.templateKey"), cell: (item) => item.templateKey },
      { id: "visibility", header: t("common.visibility"), cell: (item) => item.visibility },
      { id: "pricingModel", header: t("common.pricing"), cell: (item) => item.pricingModel },
      {
        id: "status",
        header: t("common.status"),
        cell: (item) => <span className={`status-badge status-${item.status.toLowerCase()}`}>{item.status}</span>,
      },
      { id: "updatedAt", header: t("common.updatedAt"), cell: (item) => item.updatedAt },
    ],
    [t, openVersions],
  );

  return (
    <section className="resource-page">
      <header className="page-header">
        <div>
          <span className="eyebrow">myScaffolds</span>
          <h1>{t("myScaffolds.title")}</h1>
          <p>{t("myScaffolds.description")}</p>
        </div>
        <div className="actions">
          <button className="icon-button" type="button" disabled={busy} title={t("common.refresh")} onClick={() => void load()}>
            <RefreshCw size={18} />
          </button>
          <button className="command-button" type="button" onClick={() => setDialog("create")}>
            <Plus size={15} />
            {t("myScaffolds.create")}
          </button>
        </div>
      </header>
      {error && (
        <div className="error-banner" role="alert">
          {error}
          <button className="icon-button" type="button" title={t("common.close")} onClick={() => setError(undefined)}>
            <X size={16} />
          </button>
        </div>
      )}
      {actionError && <div className="error-banner" role="alert">{actionError}</div>}
      <DataTable<ApplicationScaffoldResponse>
        columns={columns}
        density="compact"
        emptyState={<span>{t("myScaffolds.empty")}</span>}
        getRowId={(item) => item.id}
        loading={busy && items.length === 0}
        pagination={{
          hasMore,
          mode: "server",
          onPageChange: (next: number) => {
            if (!busy && next >= 1 && next !== page) setPage(next);
          },
          onPageSizeChange: () => undefined,
          page,
          pageSize: PAGE_SIZE,
          pageSizeOptions: [PAGE_SIZE],
          ...(total === 0 ? {} : { rowCount: total }),
        }}
        rows={items as ApplicationScaffoldResponse[]}
        stickyHeader
      />
      {/* The version ledger opens as a dialog over the page, not an inline
          panel below it: the row click fetches the template's versions and the
          dialog carries the per-template actions. A command dialog or the edit
          dialog takes over the backdrop while open and returns here on close. */}
      {expanded && dialog === undefined && !editTarget && (
        <div
          className="dialog-backdrop"
          role="presentation"
          onMouseDown={(event) => {
            if (event.target === event.currentTarget) setExpanded(undefined);
          }}
        >
          <div className="dialog" role="dialog" aria-modal="true" aria-labelledby="versions-dialog-title">
            <header>
              <div>
                <span className="eyebrow">{items.find((item) => item.id === expanded)?.displayName ?? expanded}</span>
                <h2 id="versions-dialog-title">{t("myScaffolds.versions")}</h2>
              </div>
              <button className="icon-button" title={t("common.close")} type="button" onClick={() => setExpanded(undefined)}>
                <X size={18} />
              </button>
            </header>
            {actionError && <div className="error-banner" role="alert">{actionError}</div>}
            {versions.length === 0 ? (
              <div className="empty-state">{t("myScaffolds.versions.empty")}</div>
            ) : (
              <ul className="version-list">
                {versions.map((version) => (
                  <li key={version.id}>
                    <span className={`status-badge status-${version.status.toLowerCase()}`}>{version.status}</span>
                    <strong>{version.templateVersion}</strong>
                    <span>{version.changelog}</span>
                  </li>
                ))}
              </ul>
            )}
            <div className="actions">
              <button className="command-button" type="button" onClick={() => setDialog("version")}>
                <Plus size={15} />
                {t("myScaffolds.addVersion")}
              </button>
              <button className="secondary-button" type="button" onClick={() => void openEdit(expanded)}>
                {t("myScaffolds.edit")}
              </button>
              <button
                className="command-button"
                type="button"
                onClick={() => void runTemplateAction((templateUuid) => service.submit(templateUuid), expanded)}
              >
                {t("myScaffolds.submit")}
              </button>
              <button
                className="danger-button"
                type="button"
                onClick={() => {
                  const templateUuid = expanded;
                  setExpanded(undefined);
                  void runTemplateAction((uuid) => service.remove(uuid), templateUuid);
                }}
              >
                {t("myScaffolds.delete")}
              </button>
            </div>
            <footer>
              <button className="secondary-button" type="button" onClick={() => setExpanded(undefined)}>
                {t("common.close")}
              </button>
            </footer>
          </div>
        </div>
      )}
      {dialog === "create" && (
        <CreateTemplateDialog
          apps={apps}
          categories={categories}
          locale={locale}
          close={() => setDialog(undefined)}
          done={() => {
            setDialog(undefined);
            void load();
          }}
          service={service}
        />
      )}
      {dialog === "version" && expanded && (
        <CreateVersionDialog
          locale={locale}
          close={() => setDialog(undefined)}
          done={() => {
            setDialog(undefined);
            void openVersions(expanded);
          }}
          service={service}
          templateUuid={expanded}
        />
      )}
      {editTarget && (
        <EditTemplateDialog
          categories={categories}
          locale={locale}
          template={editTarget}
          close={() => setEditTarget(undefined)}
          done={() => {
            setEditTarget(undefined);
            void load();
          }}
          service={service}
        />
      )}
    </section>
  );
}

function CreateTemplateDialog({
  apps,
  categories,
  close,
  done,
  locale,
  service,
}: {
  apps: readonly { id: string; name: string }[];
  categories: readonly ScaffoldCategoryResponse[];
  close(): void;
  done(): void;
  locale: DeploymentsLocale;
  service: ReturnType<typeof createMyScaffoldsService>;
}) {
  const t = marketplaceTranslator(locale);
  const [form, setForm] = useState({
    appUuid: apps[0]?.id ?? "",
    templateType: "APP" as ScaffoldType,
    categoryUuid: categories[0]?.id ?? "",
    templateKey: "",
    displayName: "",
    summary: "",
    description: "",
    visibility: "PRIVATE" as "PUBLIC" | "PRIVATE",
    pricingModel: "FREE" as "FREE" | "PAID",
    priceMinor: "",
    initialVersion: "",
  });
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>();
  async function submit(event: FormEvent): Promise<void> {
    event.preventDefault();
    if (!form.appUuid || !form.categoryUuid || !form.templateKey.trim() || !form.displayName.trim() || !form.summary.trim()) return;
    setBusy(true);
    setError(undefined);
    try {
      await service.create({
        appUuid: form.appUuid,
        templateType: form.templateType,
        categoryUuid: form.categoryUuid,
        templateKey: form.templateKey,
        displayName: form.displayName,
        summary: form.summary,
        description: form.description === "" ? undefined : form.description,
        visibility: form.visibility,
        pricingModel: form.pricingModel,
        priceMinor: form.priceMinor === "" ? undefined : form.priceMinor,
        initialVersion: form.initialVersion === "" ? undefined : form.initialVersion,
      });
      done();
    } catch {
      setError(t("myScaffolds.operationFailed"));
    } finally {
      setBusy(false);
    }
  }
  return (
    <div
      className="dialog-backdrop"
      role="presentation"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) close();
      }}
    >
      <form className="dialog" role="dialog" aria-modal="true" aria-labelledby="create-scaffold-title" onSubmit={(event) => void submit(event)}>
        <header>
          <div>
            <span className="eyebrow">{t("myScaffolds.title")}</span>
            <h2 id="create-scaffold-title">{t("myScaffolds.create")}</h2>
          </div>
          <button className="icon-button" title={t("common.close")} type="button" onClick={close}>
            <X size={18} />
          </button>
        </header>
        <div className="form-grid">
          <label>
            <span>{t("myScaffolds.create.appUuid")}</span>
            <select required value={form.appUuid} onChange={(event) => setForm((current) => ({ ...current, appUuid: event.target.value }))}>
              <option value="">{t("myScaffolds.pickApp")}</option>
              {apps.map((app) => (
                <option key={app.id} value={app.id}>
                  {app.name}
                </option>
              ))}
            </select>
          </label>
          <label>
            <span>{t("myScaffolds.create.templateType")}</span>
            <select
              value={form.templateType}
              onChange={(event) => setForm((current) => ({ ...current, templateType: event.target.value as ScaffoldType }))}
            >
              <option value="APP">{t("marketplace.type.APP")}</option>
              <option value="PPT">{t("marketplace.type.PPT")}</option>
              <option value="VIDEO">{t("marketplace.type.VIDEO")}</option>
            </select>
          </label>
          <label>
            <span>{t("myScaffolds.create.categoryUuid")}</span>
            <select required value={form.categoryUuid} onChange={(event) => setForm((current) => ({ ...current, categoryUuid: event.target.value }))}>
              <option value="">{t("myScaffolds.pickCategory")}</option>
              {categories.map((category) => (
                <option key={category.id} value={category.id}>
                  {category.displayName}
                </option>
              ))}
            </select>
          </label>
          <label>
            <span>{t("myScaffolds.create.templateKey")}</span>
            <input required maxLength={64} value={form.templateKey} onChange={(event) => setForm((current) => ({ ...current, templateKey: event.target.value }))} />
          </label>
          <label>
            <span>{t("myScaffolds.create.displayName")}</span>
            <input required maxLength={200} value={form.displayName} onChange={(event) => setForm((current) => ({ ...current, displayName: event.target.value }))} />
          </label>
          <label>
            <span>{t("myScaffolds.create.summary")}</span>
            <input required maxLength={512} value={form.summary} onChange={(event) => setForm((current) => ({ ...current, summary: event.target.value }))} />
          </label>
          <label>
            <span>{t("myScaffolds.create.description")}</span>
            <textarea value={form.description} onChange={(event) => setForm((current) => ({ ...current, description: event.target.value }))} />
          </label>
          <label>
            <span>{t("myScaffolds.create.visibility")}</span>
            <select value={form.visibility} onChange={(event) => setForm((current) => ({ ...current, visibility: event.target.value as "PUBLIC" | "PRIVATE" }))}>
              <option value="PRIVATE">PRIVATE</option>
              <option value="PUBLIC">PUBLIC</option>
            </select>
          </label>
          <label>
            <span>{t("myScaffolds.create.pricingModel")}</span>
            <select value={form.pricingModel} onChange={(event) => setForm((current) => ({ ...current, pricingModel: event.target.value as "FREE" | "PAID" }))}>
              <option value="FREE">FREE</option>
              <option value="PAID">PAID</option>
            </select>
          </label>
          {form.pricingModel === "PAID" && (
            <label>
              <span>{t("myScaffolds.create.priceMinor")}</span>
              <input inputMode="numeric" pattern="[0-9]+" required value={form.priceMinor} onChange={(event) => setForm((current) => ({ ...current, priceMinor: event.target.value }))} />
            </label>
          )}
          <label>
            <span>{t("myScaffolds.create.initialVersion")}</span>
            <input maxLength={64} value={form.initialVersion} onChange={(event) => setForm((current) => ({ ...current, initialVersion: event.target.value }))} />
          </label>
        </div>
        {error && <div className="error-banner" role="alert">{error}</div>}
        <footer>
          <button className="secondary-button" type="button" onClick={close}>
            {t("common.cancel")}
          </button>
          <button className="command-button" disabled={busy} type="submit">
            {busy ? t("common.working") : t("common.confirm")}
          </button>
        </footer>
      </form>
    </div>
  );
}

function CreateVersionDialog({
  close,
  done,
  locale,
  service,
  templateUuid,
}: {
  close(): void;
  done(): void;
  locale: DeploymentsLocale;
  service: ReturnType<typeof createMyScaffoldsService>;
  templateUuid: string;
}) {
  const t = marketplaceTranslator(locale);
  const [form, setForm] = useState({
    version: "",
    changelog: "",
    artifactUuid: "",
    sourceAppVersion: "",
    packageSizeBytes: "",
    checksumSha256: "",
    platformTargets: "",
  });
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>();
  async function submit(event: FormEvent): Promise<void> {
    event.preventDefault();
    if (!form.version.trim()) return;
    setBusy(true);
    setError(undefined);
    try {
      // Platform targets are a comma-separated list on one input: the contract
      // takes an array, and a free-text field keeps the dialog honest about
      // accepting any target string the build produced.
      const platformTargets = form.platformTargets
        .split(",")
        .map((target) => target.trim())
        .filter((target) => target !== "");
      await service.createVersion(templateUuid, {
        version: form.version,
        changelog: form.changelog === "" ? undefined : form.changelog,
        artifactUuid: form.artifactUuid === "" ? undefined : form.artifactUuid,
        sourceAppVersion: form.sourceAppVersion === "" ? undefined : form.sourceAppVersion,
        packageSizeBytes: form.packageSizeBytes === "" ? undefined : form.packageSizeBytes,
        checksumSha256: form.checksumSha256 === "" ? undefined : form.checksumSha256,
        platformTargets,
      });
      done();
    } catch {
      setError(t("myScaffolds.operationFailed"));
    } finally {
      setBusy(false);
    }
  }
  return (
    <div
      className="dialog-backdrop"
      role="presentation"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) close();
      }}
    >
      <form className="dialog" role="dialog" aria-modal="true" aria-labelledby="create-version-title" onSubmit={(event) => void submit(event)}>
        <header>
          <div>
            <span className="eyebrow">{t("myScaffolds.versions")}</span>
            <h2 id="create-version-title">{t("myScaffolds.addVersion")}</h2>
          </div>
          <button className="icon-button" title={t("common.close")} type="button" onClick={close}>
            <X size={18} />
          </button>
        </header>
        <div className="form-grid">
          <label>
            <span>{t("myScaffolds.version")}</span>
            <input required maxLength={64} value={form.version} onChange={(event) => setForm((current) => ({ ...current, version: event.target.value }))} />
          </label>
          <label>
            <span>{t("myScaffolds.changelog")}</span>
            <textarea value={form.changelog} onChange={(event) => setForm((current) => ({ ...current, changelog: event.target.value }))} />
          </label>
          <label>
            <span>{t("myScaffolds.artifactUuid")}</span>
            <input maxLength={36} value={form.artifactUuid} onChange={(event) => setForm((current) => ({ ...current, artifactUuid: event.target.value }))} />
          </label>
          <label>
            <span>{t("myScaffolds.sourceAppVersion")}</span>
            <input maxLength={64} value={form.sourceAppVersion} onChange={(event) => setForm((current) => ({ ...current, sourceAppVersion: event.target.value }))} />
          </label>
          <label>
            <span>{t("myScaffolds.packageSizeBytes")}</span>
            <input inputMode="numeric" pattern="[0-9]*" value={form.packageSizeBytes} onChange={(event) => setForm((current) => ({ ...current, packageSizeBytes: event.target.value }))} />
          </label>
          <label>
            <span>{t("myScaffolds.checksumSha256")}</span>
            <input maxLength={128} value={form.checksumSha256} onChange={(event) => setForm((current) => ({ ...current, checksumSha256: event.target.value }))} />
          </label>
          <label>
            <span>{t("myScaffolds.platformTargets")}</span>
            <input
              placeholder={t("myScaffolds.platformTargets.hint")}
              value={form.platformTargets}
              onChange={(event) => setForm((current) => ({ ...current, platformTargets: event.target.value }))}
            />
          </label>
        </div>
        {error && <div className="error-banner" role="alert">{error}</div>}
        <footer>
          <button className="secondary-button" type="button" onClick={close}>
            {t("common.cancel")}
          </button>
          <button className="command-button" disabled={busy} type="submit">
            {busy ? t("common.working") : t("common.confirm")}
          </button>
        </footer>
      </form>
    </div>
  );
}

/**
 * Listing editor for one of the caller's own templates. It is a patch dialog:
 * every field is seeded from the stored listing and sent back unchanged unless
 * the author edits it, which keeps a display-name fix from silently rewriting
 * pricing or visibility.
 */
function EditTemplateDialog({
  categories,
  close,
  done,
  locale,
  service,
  template,
}: {
  categories: readonly ScaffoldCategoryResponse[];
  close(): void;
  done(): void;
  locale: DeploymentsLocale;
  service: ReturnType<typeof createMyScaffoldsService>;
  template: ApplicationScaffoldResponse;
}) {
  const t = marketplaceTranslator(locale);
  const [form, setForm] = useState({
    displayName: template.displayName,
    summary: template.summary,
    description: template.description,
    categoryUuid: template.categoryUuid,
    visibility: template.visibility as "PUBLIC" | "PRIVATE",
    pricingModel: template.pricingModel as "FREE" | "PAID",
    priceMinor: template.pricingModel === "PAID" ? template.priceMinor : "",
    currency: template.currency,
  });
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>();
  async function submit(event: FormEvent): Promise<void> {
    event.preventDefault();
    if (!form.displayName.trim() || !form.summary.trim() || !form.categoryUuid) return;
    setBusy(true);
    setError(undefined);
    try {
      await service.update(template.id, {
        displayName: form.displayName,
        summary: form.summary,
        description: form.description,
        categoryUuid: form.categoryUuid,
        visibility: form.visibility,
        pricingModel: form.pricingModel,
        // A FREE listing must carry no price: the DDL CHECK rejects
        // "free but priced", so the field collapses to 0 on that branch.
        priceMinor: form.pricingModel === "PAID" ? form.priceMinor : "0",
        currency: form.currency,
      });
      done();
    } catch {
      setError(t("myScaffolds.updateFailed"));
    } finally {
      setBusy(false);
    }
  }
  return (
    <div
      className="dialog-backdrop"
      role="presentation"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) close();
      }}
    >
      <form className="dialog" role="dialog" aria-modal="true" aria-labelledby="edit-scaffold-title" onSubmit={(event) => void submit(event)}>
        <header>
          <div>
            <span className="eyebrow">{t("myScaffolds.title")}</span>
            <h2 id="edit-scaffold-title">{t("myScaffolds.edit")}</h2>
          </div>
          <button className="icon-button" title={t("common.close")} type="button" onClick={close}>
            <X size={18} />
          </button>
        </header>
        <div className="form-grid">
          <label>
            <span>{t("myScaffolds.create.displayName")}</span>
            <input required maxLength={200} value={form.displayName} onChange={(event) => setForm((current) => ({ ...current, displayName: event.target.value }))} />
          </label>
          <label>
            <span>{t("myScaffolds.create.summary")}</span>
            <input required maxLength={512} value={form.summary} onChange={(event) => setForm((current) => ({ ...current, summary: event.target.value }))} />
          </label>
          <label>
            <span>{t("myScaffolds.create.description")}</span>
            <textarea value={form.description} onChange={(event) => setForm((current) => ({ ...current, description: event.target.value }))} />
          </label>
          <label>
            <span>{t("myScaffolds.create.categoryUuid")}</span>
            <select required value={form.categoryUuid} onChange={(event) => setForm((current) => ({ ...current, categoryUuid: event.target.value }))}>
              <option value="">{t("myScaffolds.pickCategory")}</option>
              {categories.map((category) => (
                <option key={category.id} value={category.id}>
                  {category.displayName}
                </option>
              ))}
            </select>
          </label>
          <label>
            <span>{t("myScaffolds.create.visibility")}</span>
            <select value={form.visibility} onChange={(event) => setForm((current) => ({ ...current, visibility: event.target.value as "PUBLIC" | "PRIVATE" }))}>
              <option value="PRIVATE">PRIVATE</option>
              <option value="PUBLIC">PUBLIC</option>
            </select>
          </label>
          <label>
            <span>{t("myScaffolds.create.pricingModel")}</span>
            <select value={form.pricingModel} onChange={(event) => setForm((current) => ({ ...current, pricingModel: event.target.value as "FREE" | "PAID" }))}>
              <option value="FREE">FREE</option>
              <option value="PAID">PAID</option>
            </select>
          </label>
          {form.pricingModel === "PAID" && (
            <label>
              <span>{t("myScaffolds.create.priceMinor")}</span>
              <input inputMode="numeric" pattern="[0-9]+" required value={form.priceMinor} onChange={(event) => setForm((current) => ({ ...current, priceMinor: event.target.value }))} />
            </label>
          )}
          <label>
            <span>{t("myScaffolds.create.currency")}</span>
            <input maxLength={8} value={form.currency} onChange={(event) => setForm((current) => ({ ...current, currency: event.target.value }))} />
          </label>
        </div>
        {error && <div className="error-banner" role="alert">{error}</div>}
        <footer>
          <button className="secondary-button" type="button" onClick={close}>
            {t("common.cancel")}
          </button>
          <button className="command-button" disabled={busy} type="submit">
            {busy ? t("common.working") : t("common.confirm")}
          </button>
        </footer>
      </form>
    </div>
  );
}
