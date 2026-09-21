import { Activity, AppWindow, Boxes, FileKey2, FolderTree, Globe2, LogOut, Network, Package, RefreshCw, Rocket, ScrollText, Search, Server, ServerCog, Settings2, Shield, Tags, Upload, X } from "lucide-react";
import { Suspense, useEffect, useMemo, useState, type FormEvent, type ReactNode } from "react";
import { NavLink, Navigate, Route, Routes } from "react-router-dom";

import { DataTable, type DataTableColumn } from "@sdkwork/ui-pc-react";

import { translateDeployments, type DeploymentsLocale, type DeploymentsMessageKey } from "./i18n/index.ts";
import type { DeploymentsAction, DeploymentsDataSource, DeploymentsModuleEntry, DeploymentsPcModuleDefinition, DeploymentsRegistry, DeploymentsResourceKey, DeploymentsResourcePages } from "./types.ts";

/** 通用资源表的每页条数选项；服务端分页时页码/总数由后端 pageInfo 决定。 */
const RESOURCE_PAGE_SIZES = [20, 50, 100] as const;

export interface DeploymentsWorkspaceProps {
  locale: DeploymentsLocale;
  modules: readonly DeploymentsPcModuleDefinition[];
  onSignOut?: (() => void) | undefined;
  permissionScope: readonly string[];
  registry: DeploymentsRegistry;
  resourcePages?: DeploymentsResourcePages | undefined;
  surface: "app-console" | "backend-admin";
  userLabel?: string | undefined;
}

export function DeploymentsWorkspace({ locale, modules, onSignOut, permissionScope, registry, resourcePages, surface, userLabel }: DeploymentsWorkspaceProps) {
  const t = translator(locale);
  const entries = useMemo(() => modules.flatMap((module) => module.entries).filter((entry) => permissionScope.length === 0 || !entry.permission || permissionScope.includes(entry.permission)).sort((a, b) => a.order - b.order), [modules, permissionScope]);
  // Destructuring keeps the "at least one visible entry" invariant in the
  // types: `entries[0]` would widen to `DeploymentsModuleEntry | undefined`
  // under `noUncheckedIndexedAccess` and push a non-null assertion down into
  // the default redirect below.
  const [firstEntry] = entries;
  const base = surface === "backend-admin" ? "/admin" : "/console";
  if (!firstEntry) return <main className="empty-access" role="alert"><Shield size={22} /><h1>{t("access.title")}</h1><p>{t("access.description")}</p></main>;
  return <div className="app-layout">
    <aside className="sidebar"><div className="brand"><span className="brand-mark"><Boxes size={19} /></span><div><strong>{t("brand.name")}</strong><small>{t(`surface.${surface}`)}</small></div></div><nav aria-label={t("nav.primary")}>{entries.map((entry) => <NavLink key={entry.resource} to={`${base}/${entry.resource}`} title={resourceText(t, entry.resource, "label")}><span className="nav-icon">{resourceIcon(entry.resource)}</span><span className="nav-label">{resourceText(t, entry.resource, "label")}</span></NavLink>)}</nav><div className="sidebar-footer"><span title={userLabel}>{userLabel ?? t("auth.user")}</span>{onSignOut && <button className="icon-button" type="button" title={t("auth.signOut")} onClick={onSignOut}><LogOut size={17} /></button>}</div></aside>
    <main className="workspace"><Routes>{entries.map((entry) => {
      const ResourcePage = resourcePages?.[entry.resource];
      return <Route key={entry.resource} path={`${entry.resource}/*`} element={ResourcePage ? <Suspense fallback={<div className="resource-loading" aria-busy="true"><RefreshCw size={20} /></div>}><ResourcePage locale={locale} /></Suspense> : <Page entry={entry} locale={locale} source={registry[entry.resource]} />} />;
    })}<Route path="*" element={<Navigate to={`${base}/${firstEntry.resource}`} replace />} /></Routes></main>
  </div>;
}

function Page({ entry, locale, source }: { entry: DeploymentsModuleEntry; locale: DeploymentsLocale; source?: DeploymentsDataSource | undefined }) {
  const t = translator(locale);
  const [items, setItems] = useState<readonly Record<string, unknown>[]>([]);
  const [page, setPage] = useState(1);
  const [pageSize, setPageSize] = useState<number>(RESOURCE_PAGE_SIZES[0]);
  const [pageInfo, setPageInfo] = useState<{ page: number; pageSize: number; hasMore: boolean; total: number | undefined }>({ page: 1, pageSize: RESOURCE_PAGE_SIZES[0], hasMore: false, total: undefined });
  const [scopeId, setScopeId] = useState(() => sessionStorage.getItem("sdkwork.deployments.siteId") ?? "");
  const [search, setSearch] = useState("");
  const [selected, setSelected] = useState<Record<string, unknown>>();
  const [action, setAction] = useState<DeploymentsAction>();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>();

  async function load(): Promise<void> {
    if (!source || (source.requiresScope && !scopeId.trim())) { setItems([]); return; }
    setBusy(true); setError(undefined);
    try {
      const result = await source.load({ page, pageSize, scopeId: scopeId.trim() || undefined, search: search.trim() || undefined });
      setItems(result.items); setPageInfo({ ...result.pageInfo, total: result.pageInfo.total });
    } catch { setError(t("error.operation")); } finally { setBusy(false); }
  }

  useEffect(() => { void load(); }, [entry.resource, page, pageSize, scopeId]);
  useEffect(() => { setPage(1); setSelected(undefined); }, [entry.resource]);
  const columns = useMemo(() => Array.from(new Set(items.flatMap(Object.keys))).slice(0, 7), [items]);
  const showsScope = source?.requiresScope || source?.actions.some((candidate) => candidate.requiresScope);
  const updateScope = (value: string) => { setScopeId(value); if (value.trim()) sessionStorage.setItem("sdkwork.deployments.siteId", value.trim()); else sessionStorage.removeItem("sdkwork.deployments.siteId"); };

  /**
   * 表列：由当前页实际出现的字段派生（原实现同样如此），渲染复用 `display()`。
   * 选择列交给框架的 selectable，不再手写 radio 单元格。
   */
  const tableColumns = useMemo<DataTableColumn<Record<string, unknown>>[]>(
    () => columns.map((column) => ({
      id: column,
      header: humanize(column),
      cell: (item: Record<string, unknown>) => display(item[column], column),
    })),
    [columns],
  );

  /**
   * 分页：后端 `pageInfo` 有两种形态 —— 带 `total` 的 offset 分页（可算页数）
   * 与只带 `hasMore` 的 cursor 分页（无总数）。后者把 `hasMore` 交给框架，
   * 由框架渲染页码序数并用该标志驱动「下一页」。
   *
   * `rowCount` 在无总数时必须**缺席**（而非 `undefined`）：框架用「`hasMore`
   * 是否存在」判定 cursor 模式，用 `rowCount` 判定 offset 模式，传 `undefined`
   * 会在 `exactOptionalPropertyTypes` 下被拒。
   */
  const pagination = {
    hasMore: pageInfo.hasMore,
    mode: "server" as const,
    onPageChange: (next: number) => {
      if (busy || next < 1 || next === page) return;
      if (pageInfo.total === undefined && next !== page + 1 && next !== page - 1) return;
      setPage(next);
    },
    onPageSizeChange: (next: number) => {
      if (busy || next === pageSize) return;
      setPageSize(next); setPage(1);
    },
    page: pageInfo.total === undefined ? page : pageInfo.page,
    pageSize: pageInfo.pageSize,
    pageSizeOptions: RESOURCE_PAGE_SIZES,
    ...(pageInfo.total === undefined ? {} : { rowCount: pageInfo.total }),
  };

  return <section className="resource-page">
    <header className="page-header"><div><span className="eyebrow">{entry.resource}</span><h1>{resourceText(t, entry.resource, "label")}</h1><p>{resourceText(t, entry.resource, "description")}</p></div><button className="icon-button" type="button" disabled={busy} title={t("toolbar.refresh")} onClick={() => void load()}><RefreshCw size={18} /></button></header>
    <div className="toolbar"><form className="search-box" onSubmit={(event) => { event.preventDefault(); setPage(1); void load(); }}><Search size={16} /><input aria-label={t("toolbar.search")} value={search} onChange={(event) => setSearch(event.target.value)} placeholder={t("toolbar.search")} /></form>{showsScope && <label className="scope-input"><Settings2 size={16} /><input aria-label={t("toolbar.siteId")} value={scopeId} onChange={(event) => updateScope(event.target.value)} placeholder={t("toolbar.siteId")} /></label>}<div className="actions">{source?.actions.map((candidate) => <button key={candidate.id} className={candidate.dangerous ? "danger-button" : "command-button"} disabled={busy || (candidate.requiresSelection && !selected) || (candidate.requiresScope && !scopeId.trim())} onClick={() => setAction(candidate)} type="button">{candidate.requiresFile && <Upload size={15} />}{actionText(t, entry.resource, candidate)}</button>)}</div></div>
    {error && <div className="error-banner" role="alert">{error}<button className="icon-button" type="button" title={t("toolbar.dismiss")} onClick={() => setError(undefined)}><X size={16} /></button></div>}
    {source?.requiresScope && !scopeId.trim()
      ? <div className="empty-state">{t("scope.empty")}</div>
      : <DataTable<Record<string, unknown>>
          columns={tableColumns}
          density="compact"
          emptyState={<span>{t("table.empty")}</span>}
          getRowId={(item, index) => recordKey(item, index)}
          getRowSelectionLabel={(_item, index) => t("table.selectRow", { row: index + 1 })}
          loading={busy && items.length === 0}
          onSelectedRowIdsChange={(ids) => {
            const [nextId] = ids;
            setSelected(nextId === undefined ? undefined : items.find((item, index) => recordKey(item, index) === String(nextId)));
          }}
          pagination={pagination}
          rows={items as Record<string, unknown>[]}
          selectable
          selectedRowIds={selected === undefined ? [] : [recordKey(selected, items.indexOf(selected))]}
          stickyHeader
        />}
    {action && <Dialog action={action} label={actionText(t, entry.resource, action)} locale={locale} scopeId={scopeId || undefined} selected={selected} close={() => setAction(undefined)} done={() => { setAction(undefined); void load(); }} />}
  </section>;
}

function Dialog({ action, close, done, label, locale, scopeId, selected }: { action: DeploymentsAction; close(): void; done(): void; label: string; locale: DeploymentsLocale; scopeId?: string | undefined; selected?: Record<string, unknown> | undefined }) {
  const t = translator(locale);
  const [body, setBody] = useState<Record<string, unknown>>(() => ({ ...action.bodyTemplate }));
  const [file, setFile] = useState<File>();
  const [confirmed, setConfirmed] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>();
  async function submit(event: FormEvent): Promise<void> {
    event.preventDefault();
    if ((action.requiresFile && !file) || (action.dangerous && !confirmed)) return;
    setBusy(true); setError(undefined);
    try { await action.execute({ body, file, scopeId, selectedItem: selected }); done(); }
    catch { setError(t("error.operation")); } finally { setBusy(false); }
  }
  return <div className="dialog-backdrop" role="presentation" onMouseDown={(event) => { if (event.target === event.currentTarget) close(); }}><form className="dialog" role="dialog" aria-modal="true" aria-labelledby="action-title" onSubmit={(event) => void submit(event)}><header><div><span className="eyebrow">{t("dialog.command")}</span><h2 id="action-title">{label}</h2></div><button className="icon-button" title={t("dialog.close")} type="button" onClick={close}><X size={18} /></button></header>{action.dangerous && <div className="warning">{t("dialog.warning")}</div>}{action.requiresFile && <label><span>{t("dialog.file")}</span><input type="file" accept=".zip,.tar,.gz,.tgz" onChange={(event) => setFile(event.target.files?.[0])} /></label>}<div className="form-grid">{Object.entries(body).map(([name, value]) => <Field key={name} name={name} value={value} onChange={(next) => setBody((current) => ({ ...current, [name]: next }))} />)}</div>{action.dangerous && <label className="confirm-check"><input type="checkbox" checked={confirmed} onChange={(event) => setConfirmed(event.target.checked)} />{t("dialog.confirmRisk")}</label>}{error && <div className="error-banner" role="alert">{error}</div>}<footer><button className="secondary-button" type="button" onClick={close}>{t("dialog.cancel")}</button><button className={action.dangerous ? "danger-button" : "command-button"} disabled={busy || Boolean(action.requiresFile && !file) || Boolean(action.dangerous && !confirmed)}>{busy ? t("dialog.submitting") : t("dialog.confirm")}</button></footer></form></div>;
}

function Field({ name, onChange, value }: { name: string; onChange(value: unknown): void; value: unknown }) {
  if (typeof value === "boolean") return <label className="checkbox-field"><input type="checkbox" checked={value} onChange={(event) => onChange(event.target.checked)} /><span>{humanize(name)}</span></label>;
  if (typeof value === "number") return <label><span>{humanize(name)}</span><input type="number" value={value} onChange={(event) => onChange(Number(event.target.value))} /></label>;
  const multiline = /content|description|value/i.test(name);
  return <label><span>{humanize(name)}</span>{multiline ? <textarea value={String(value ?? "")} onChange={(event) => onChange(event.target.value)} /> : <input type={sensitive(name) ? "password" : "text"} value={String(value ?? "")} onChange={(event) => onChange(event.target.value)} autoComplete="off" />}</label>;
}

function translator(locale: DeploymentsLocale) { return (key: DeploymentsMessageKey, values?: Record<string, string | number>) => translateDeployments(locale, key, values); }
function resourceText(t: ReturnType<typeof translator>, resource: DeploymentsResourceKey, field: "label" | "description"): string { return t(`resource.${resource}.${field}` as DeploymentsMessageKey); }
function actionText(t: ReturnType<typeof translator>, resource: DeploymentsResourceKey, action: DeploymentsAction): string { const key = `action.${resource}.${action.id}` as DeploymentsMessageKey; try { return t(key); } catch { return action.label; } }
function recordKey(item: Record<string, unknown>, index: number): string { return String(item.id ?? item.siteId ?? item.domainId ?? item.certificateId ?? item.deploymentId ?? item.configId ?? item.serverId ?? index); }
function display(value: unknown, column: string): ReactNode { if (value === undefined || value === null) return "-"; if (column.toLowerCase().includes("status")) return <span className={`status-badge status-${String(value).toLowerCase()}`}>{String(value)}</span>; return typeof value === "object" ? JSON.stringify(value) : String(value); }
function humanize(value: string): string { return value.replace(/([a-z])([A-Z])/g, "$1 $2").replaceAll("_", " "); }
function sensitive(value: string): boolean { return /secret|password|token|private|key/i.test(value); }
function resourceIcon(resource: DeploymentsResourceKey): ReactNode {
  const icons = {
    configuration: Settings2,
    domains: Globe2,
    certificates: FileKey2,
    apps: AppWindow,
    artifacts: Package,
    releases: Tags,
    deployments: Rocket,
    monitoring: Activity,
    nginx: ServerCog,
    clusters: Network,
    nodes: Server,
    audit: ScrollText,
    localProjects: FolderTree,
  } satisfies Record<DeploymentsResourceKey, typeof AppWindow>;
  const Icon = icons[resource];
  return <Icon size={17} />;
}
