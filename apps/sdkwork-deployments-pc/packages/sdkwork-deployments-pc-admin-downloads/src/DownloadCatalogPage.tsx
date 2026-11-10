//! Download catalog editor.
//!
//! One page owns the whole tree the public download endpoint projects:
//! catalog identity, the cards the page renders, the installers inside each card,
//! and the per-installer update configuration. The page is deliberately a
//! single editor rather than three registry tables: the update configuration only
//! means something relative to one installer of one card of one catalog, and
//! splitting it across routes would make the operator re-select the same path
//! three times.
//!
//! The client comes from the admin context (`useDeploymentsAdminClient`), so the
//! package stays host-agnostic and needs no props beyond the locale.

import { useCallback, useEffect, useMemo, useState } from "react";
import { useDeploymentsAdminClient } from "@sdkwork/deployments-pc-admin-core";
import type { DeploymentsLocale } from "@sdkwork/deployments-pc-commons";
import type {
  DownloadCardResponse,
  DownloadCatalogResponse,
  DownloadCatalogSummaryResponse,
  DownloadItemUpsertRequest,
  DownloadItemResponse,
  DownloadUpdateResponse,
  DownloadUpdateUpsertRequest,
} from "@sdkwork/deployments-backend-sdk";

import { createDownloadsAdminService } from "./service.ts";

export interface DownloadCatalogPageProps {
  locale: DeploymentsLocale;
}

const PROVIDERS = ["deployments", "generic", "github", "none"] as const;
const CHANNELS = ["nightly", "stable"] as const;
const PLATFORMS = [
  "windows",
  "macos",
  "linux",
  "android",
  "ios",
  "harmonyos",
  "docker",
  "helm",
  "generic",
] as const;
const ARCHITECTURES = ["x64", "arm64", "universal"] as const;

type Copy = ReturnType<typeof copyOf>;

function copyOf(locale: DeploymentsLocale) {
  if (locale === "zh-CN") {
    return {
      title: "下载目录",
      subtitle: "下载页卡片、安装包，以及每个安装包自己的更新配置。",
      catalogs: "目录",
      newCatalog: "新建目录",
      catalogKey: "目录标识",
      productKey: "产品标识",
      productName: "产品名称",
      productVersion: "产品版本",
      defaultChannel: "默认渠道",
      status: "状态",
      revision: "修订号",
      save: "保存",
      publish: "发布",
      published: "已发布",
      draft: "草稿",
      cards: "卡片",
      addCard: "新增卡片",
      cardKey: "卡片标识",
      cardTitle: "标题",
      cardDescription: "描述",
      hidden: "隐藏",
      order: "排序",
      items: "安装包",
      addItem: "新增安装包",
      itemKey: "安装包标识",
      platform: "平台",
      architecture: "架构",
      label: "按钮文案",
      fileName: "文件名",
      version: "版本",
      downloadUrl: "下载地址",
      disabled: "停用",
      primary: "主按钮",
      updateConfig: "更新配置",
      provider: "更新来源",
      feedUrl: "外部 Feed 地址",
      channel: "渠道",
      minVersion: "最低支持版本",
      interval: "检查间隔（分钟）",
      rollout: "灰度比例（%）",
      allowPrerelease: "允许预发布",
      allowDowngrade: "允许降级",
      saveUpdate: "保存更新配置",
      delete: "删除",
      loading: "正在加载…",
      empty: "还没有目录。先新建一个。",
      emptyCards: "还没有卡片。",
      emptyItems: "这张卡片还没有安装包。",
      saved: "已保存",
      failed: "操作失败",
      confirmDelete: "确认删除？",
    };
  }
  return {
    title: "Download catalog",
    subtitle: "Download page cards, installers, and each installer's own update feed.",
    catalogs: "Catalogs",
    newCatalog: "New catalog",
    catalogKey: "Catalog key",
    productKey: "Product key",
    productName: "Product name",
    productVersion: "Product version",
    defaultChannel: "Default channel",
    status: "Status",
    revision: "Revision",
    save: "Save",
    publish: "Publish",
    published: "Published",
    draft: "Draft",
    cards: "Cards",
    addCard: "Add card",
    cardKey: "Card key",
    cardTitle: "Title",
    cardDescription: "Description",
    hidden: "Hidden",
    order: "Order",
    items: "Installers",
    addItem: "Add installer",
    itemKey: "Installer key",
    platform: "Platform",
    architecture: "Architecture",
    label: "Button label",
    fileName: "File name",
    version: "Version",
    downloadUrl: "Download URL",
    disabled: "Disabled",
    primary: "Primary",
    updateConfig: "Update configuration",
    provider: "Provider",
    feedUrl: "External feed URL",
    channel: "Channel",
    minVersion: "Minimum supported version",
    interval: "Check interval (minutes)",
    rollout: "Rollout (%)",
    allowPrerelease: "Allow prerelease",
    allowDowngrade: "Allow downgrade",
    saveUpdate: "Save update configuration",
    delete: "Delete",
    loading: "Loading…",
    empty: "No catalogs yet. Create one first.",
    emptyCards: "No cards yet.",
    emptyItems: "This card has no installers yet.",
    saved: "Saved",
    failed: "Operation failed",
    confirmDelete: "Delete?",
  };
}

interface ItemDraft {
  platform: string;
  architecture: string;
  label: string;
  fileName: string;
  version: string;
  href: string;
  sizeBytes: string;
  checksumSha256: string;
  disabled: boolean;
  primaryAction: boolean;
}

interface UpdateDraft {
  enabled: boolean;
  provider: string;
  feedUrl: string;
  channelKey: string;
  minimumSupportedVersion: string;
  checkIntervalMinutes: string;
  rolloutPercentage: string;
  allowPrerelease: boolean;
  allowDowngrade: boolean;
}

interface CardDraft {
  title: string;
  description: string;
  hidden: boolean;
  sortOrder: string;
}

function itemDraft(item: DownloadItemResponse): ItemDraft {
  const href = typeof item.downloadSource?.href === "string" ? item.downloadSource.href : "";
  return {
    platform: item.platform,
    architecture: item.architecture,
    label: item.label,
    fileName: item.fileName ?? "",
    version: item.version ?? "",
    href,
    sizeBytes: item.sizeBytes === undefined ? "" : String(item.sizeBytes),
    checksumSha256: item.checksumSha256 ?? "",
    disabled: item.disabled,
    primaryAction: item.primaryAction,
  };
}

function updateDraft(update: DownloadUpdateResponse | undefined): UpdateDraft {
  return {
    enabled: update?.enabled ?? true,
    provider: update?.provider ?? "deployments",
    feedUrl: update?.feedUrl ?? "",
    channelKey: update?.channelKey ?? "nightly",
    minimumSupportedVersion: update?.minimumSupportedVersion ?? "",
    checkIntervalMinutes: String(update?.checkIntervalMinutes ?? 10),
    rolloutPercentage: String(update?.rolloutPercentage ?? 100),
    allowPrerelease: update?.allowPrerelease ?? true,
    allowDowngrade: update?.allowDowngrade ?? false,
  };
}

function cardDraft(card: DownloadCardResponse): CardDraft {
  return {
    title: card.title,
    description: card.description ?? "",
    hidden: card.hidden,
    sortOrder: String(card.sortOrder),
  };
}

function message(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/**
 * Renders the download catalog editor.
 * @param props - admin resource page props (locale only).
 * @returns The editor tree.
 */
export function DownloadCatalogPage({ locale }: DownloadCatalogPageProps) {
  const t: Copy = useMemo(() => copyOf(locale), [locale]);
  const client = useDeploymentsAdminClient();
  const service = useMemo(() => createDownloadsAdminService(client), [client]);

  const [catalogs, setCatalogs] = useState<DownloadCatalogSummaryResponse[]>([]);
  const [selected, setSelected] = useState<string | null>(null);
  const [catalog, setCatalog] = useState<DownloadCatalogResponse | null>(null);
  const [status, setStatus] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [newCatalogKey, setNewCatalogKey] = useState("");
  const [newCardKey, setNewCardKey] = useState("");
  const [newItemKeys, setNewItemKeys] = useState<Record<string, string>>({});
  const [product, setProduct] = useState<{
    productKey: string;
    productName: string;
    productVersion: string;
    defaultChannelKey: "nightly" | "stable";
  }>({
    productKey: "",
    productName: "",
    productVersion: "",
    defaultChannelKey: "nightly",
  });
  const [cardDrafts, setCardDrafts] = useState<Record<string, CardDraft>>({});
  const [itemDrafts, setItemDrafts] = useState<Record<string, ItemDraft>>({});
  const [updateDrafts, setUpdateDrafts] = useState<Record<string, UpdateDraft>>({});

  const loadList = useCallback(async () => {
    const rows = await service.list();
    setCatalogs(rows);
    setSelected((current) => current ?? rows[0]?.catalogKey ?? null);
  }, [service]);

  const loadCatalog = useCallback(
    async (catalogKey: string) => {
      const next = await service.retrieve(catalogKey);
      setCatalog(next);
      setProduct({
        productKey: next.productKey,
        productName: next.productName,
        productVersion: next.productVersion,
        defaultChannelKey: next.defaultChannelKey,
      });
      const cards: Record<string, CardDraft> = {};
      const items: Record<string, ItemDraft> = {};
      const updates: Record<string, UpdateDraft> = {};
      for (const card of next.cards ?? []) {
        cards[card.cardKey] = cardDraft(card);
        for (const item of card.items ?? []) {
          items[`${card.cardKey}/${item.itemKey}`] = itemDraft(item);
          updates[`${card.cardKey}/${item.itemKey}`] = updateDraft(item.update);
        }
      }
      setCardDrafts(cards);
      setItemDrafts(items);
      setUpdateDrafts(updates);
    },
    [service],
  );

  useEffect(() => {
    let live = true;
    setBusy(true);
    loadList()
      .catch((error: unknown) => {
        if (live) setStatus(`${t.failed}: ${message(error)}`);
      })
      .finally(() => {
        if (live) setBusy(false);
      });
    return () => {
      live = false;
    };
  }, [loadList, t.failed]);

  useEffect(() => {
    if (selected === null) return;
    let live = true;
    setBusy(true);
    loadCatalog(selected)
      .catch((error: unknown) => {
        if (live) setStatus(`${t.failed}: ${message(error)}`);
      })
      .finally(() => {
        if (live) setBusy(false);
      });
    return () => {
      live = false;
    };
  }, [selected, loadCatalog, t.failed]);

  const run = useCallback(
    async (operation: () => Promise<unknown>, done?: () => Promise<void> | void) => {
      setBusy(true);
      setStatus(null);
      try {
        await operation();
        if (done !== undefined) await done();
        setStatus(t.saved);
      } catch (error) {
        setStatus(`${t.failed}: ${message(error)}`);
      } finally {
        setBusy(false);
      }
    },
    [t.failed, t.saved],
  );

  const reload = useCallback(async () => {
    await loadList();
    if (selected !== null) await loadCatalog(selected);
  }, [loadCatalog, loadList, selected]);

  const patchCard = (cardKey: string, patch: Partial<CardDraft>) =>
    setCardDrafts((current) => ({ ...current, [cardKey]: { ...current[cardKey]!, ...patch } }));
  const patchItem = (key: string, patch: Partial<ItemDraft>) =>
    setItemDrafts((current) => ({ ...current, [key]: { ...current[key]!, ...patch } }));
  const patchUpdate = (key: string, patch: Partial<UpdateDraft>) =>
    setUpdateDrafts((current) => ({ ...current, [key]: { ...current[key]!, ...patch } }));

  /** Builds the installer request; optional fields are only sent when set. */
  const itemBody = (draft: ItemDraft): DownloadItemUpsertRequest => {
    const body: DownloadItemUpsertRequest = {
      platform: draft.platform as NonNullable<DownloadItemUpsertRequest["platform"]>,
      architecture: draft.architecture as NonNullable<DownloadItemUpsertRequest["architecture"]>,
      label: draft.label,
      fileName: draft.fileName,
      version: draft.version,
      downloadSource: draft.href.trim() === "" ? {} : { href: draft.href.trim() },
      disabled: draft.disabled,
      primaryAction: draft.primaryAction,
    };
    if (draft.sizeBytes.trim() !== "") body.sizeBytes = Number(draft.sizeBytes);
    if (draft.checksumSha256.trim() !== "") body.checksumSha256 = draft.checksumSha256;
    return body;
  };

  /** Builds the update-configuration request from the editor draft. */
  const updateBody = (draft: UpdateDraft): DownloadUpdateUpsertRequest => ({
    enabled: draft.enabled,
    provider: draft.provider as NonNullable<DownloadUpdateUpsertRequest["provider"]>,
    feedUrl: draft.feedUrl,
    channelKey: draft.channelKey as NonNullable<DownloadUpdateUpsertRequest["channelKey"]>,
    allowedChannels: [
      draft.channelKey as NonNullable<
        DownloadUpdateUpsertRequest["allowedChannels"]
      >[number],
    ],
    minimumSupportedVersion: draft.minimumSupportedVersion,
    checkIntervalMinutes: Number(draft.checkIntervalMinutes) || 10,
    rolloutPercentage: Number(draft.rolloutPercentage) || 0,
    allowPrerelease: draft.allowPrerelease,
    allowDowngrade: draft.allowDowngrade,
  });

  return (
    <div className="flex min-h-0 flex-col gap-4 p-6 text-sm text-slate-700">
      <header className="flex flex-wrap items-center justify-between gap-2">
        <div>
          <h1 className="text-lg font-semibold text-slate-900">{t.title}</h1>
          <p className="text-xs text-slate-500">{t.subtitle}</p>
        </div>
        {busy ? <span className="text-xs text-slate-400">{t.loading}</span> : null}
        {status !== null ? <span className="text-xs text-slate-500">{status}</span> : null}
      </header>

      <div className="flex min-h-0 gap-4">
        <aside className="w-64 shrink-0 rounded-md border border-slate-200 p-3">
          <div className="mb-2 text-xs font-semibold uppercase text-slate-500">{t.catalogs}</div>
          <ul className="mb-3 space-y-1">
            {catalogs.map((row) => (
              <li key={row.catalogKey}>
                <button
                  type="button"
                  className={`w-full truncate rounded px-2 py-1 text-left ${
                    row.catalogKey === selected ? "bg-slate-900 text-white" : "hover:bg-slate-100"
                  }`}
                  onClick={() => setSelected(row.catalogKey)}
                >
                  {row.catalogKey}
                </button>
              </li>
            ))}
          </ul>
          {catalogs.length === 0 ? <p className="mb-2 text-xs text-slate-400">{t.empty}</p> : null}
          <div className="flex gap-1">
            <input
              className="w-full rounded border border-slate-300 px-2 py-1 text-xs"
              placeholder={t.catalogKey}
              value={newCatalogKey}
              onChange={(event) => setNewCatalogKey(event.target.value)}
            />
            <button
              type="button"
              className="rounded bg-slate-900 px-2 py-1 text-xs text-white"
              disabled={busy || newCatalogKey.trim() === ""}
              onClick={() =>
                void run(
                  () => service.upsertCatalog(newCatalogKey.trim(), {}),
                  async () => {
                    setNewCatalogKey("");
                    await loadList();
                    setSelected(newCatalogKey.trim());
                  },
                )
              }
            >
              {t.newCatalog}
            </button>
          </div>
        </aside>

        <section className="min-w-0 flex-1 space-y-4">
          {catalog === null ? null : (
            <>
              <div className="rounded-md border border-slate-200 p-3">
                <div className="mb-2 flex items-center justify-between">
                  <span className="text-xs font-semibold uppercase text-slate-500">
                    {catalog.catalogKey}
                  </span>
                  <span className="text-xs text-slate-500">
                    {catalog.status === "PUBLISHED" ? t.published : t.draft} · {t.revision}{" "}
                    {catalog.revision}
                  </span>
                </div>
                <div className="grid grid-cols-2 gap-2 md:grid-cols-4">
                  <Field label={t.productKey}>
                    <input
                      className="w-full rounded border border-slate-300 px-2 py-1"
                      value={product.productKey}
                      onChange={(event) =>
                        setProduct((current) => ({ ...current, productKey: event.target.value }))
                      }
                    />
                  </Field>
                  <Field label={t.productName}>
                    <input
                      className="w-full rounded border border-slate-300 px-2 py-1"
                      value={product.productName}
                      onChange={(event) =>
                        setProduct((current) => ({ ...current, productName: event.target.value }))
                      }
                    />
                  </Field>
                  <Field label={t.productVersion}>
                    <input
                      className="w-full rounded border border-slate-300 px-2 py-1"
                      value={product.productVersion}
                      onChange={(event) =>
                        setProduct((current) => ({ ...current, productVersion: event.target.value }))
                      }
                    />
                  </Field>
                  <Field label={t.defaultChannel}>
                    <select
                      className="w-full rounded border border-slate-300 px-2 py-1"
                      value={product.defaultChannelKey}
                      onChange={(event) =>
                        setProduct((current) => ({
                          ...current,
                          defaultChannelKey: event.target.value as "nightly" | "stable",
                        }))
                      }
                    >
                      {CHANNELS.map((channel) => (
                        <option key={channel} value={channel}>
                          {channel}
                        </option>
                      ))}
                    </select>
                  </Field>
                </div>
                <div className="mt-3 flex gap-2">
                  <button
                    type="button"
                    className="rounded bg-slate-900 px-3 py-1 text-xs text-white"
                    disabled={busy}
                    onClick={() => void run(() => service.upsertCatalog(catalog.catalogKey, product), reload)}
                  >
                    {t.save}
                  </button>
                  <button
                    type="button"
                    className="rounded border border-slate-300 px-3 py-1 text-xs"
                    disabled={busy}
                    onClick={() => void run(() => service.publish(catalog.catalogKey), reload)}
                  >
                    {t.publish}
                  </button>
                </div>
              </div>

              <div className="space-y-3">
                {(catalog.cards ?? []).map((card) => {
                  const draft = cardDrafts[card.cardKey] ?? cardDraft(card);
                  return (
                    <div key={card.cardKey} className="rounded-md border border-slate-200 p-3">
                      <div className="mb-2 flex items-center justify-between">
                        <span className="text-xs font-semibold uppercase text-slate-500">
                          {card.cardKey}
                        </span>
                        <div className="flex gap-2">
                          <button
                            type="button"
                            className="rounded border border-slate-300 px-2 py-1 text-xs"
                            disabled={busy}
                            onClick={() =>
                              void run(
                                () =>
                                  service.upsertCard(catalog.catalogKey, card.cardKey, {
                                    title: draft.title,
                                    description: draft.description,
                                    hidden: draft.hidden,
                                    sortOrder: Number(draft.sortOrder) || 0,
                                  }),
                                reload,
                              )
                            }
                          >
                            {t.save}
                          </button>
                          <button
                            type="button"
                            className="rounded border border-red-300 px-2 py-1 text-xs text-red-600"
                            disabled={busy}
                            onClick={() => {
                              if (!window.confirm(t.confirmDelete)) return;
                              void run(
                                () => service.deleteCard(catalog.catalogKey, card.cardKey),
                                reload,
                              );
                            }}
                          >
                            {t.delete}
                          </button>
                        </div>
                      </div>
                      <div className="grid grid-cols-2 gap-2 md:grid-cols-4">
                        <Field label={t.cardTitle}>
                          <input
                            className="w-full rounded border border-slate-300 px-2 py-1"
                            value={draft.title}
                            onChange={(event) => patchCard(card.cardKey, { title: event.target.value })}
                          />
                        </Field>
                        <Field label={t.cardDescription}>
                          <input
                            className="w-full rounded border border-slate-300 px-2 py-1"
                            value={draft.description}
                            onChange={(event) =>
                              patchCard(card.cardKey, { description: event.target.value })
                            }
                          />
                        </Field>
                        <Field label={t.order}>
                          <input
                            className="w-full rounded border border-slate-300 px-2 py-1"
                            value={draft.sortOrder}
                            onChange={(event) =>
                              patchCard(card.cardKey, { sortOrder: event.target.value })
                            }
                          />
                        </Field>
                        <Field label={t.hidden}>
                          <input
                            type="checkbox"
                            checked={draft.hidden}
                            onChange={(event) =>
                              patchCard(card.cardKey, { hidden: event.target.checked })
                            }
                          />
                        </Field>
                      </div>

                      <div className="mt-3">
                        <div className="mb-2 text-xs font-semibold uppercase text-slate-500">
                          {t.items}
                        </div>
                        {(card.items ?? []).length === 0 ? (
                          <p className="text-xs text-slate-400">{t.emptyItems}</p>
                        ) : null}
                        <div className="space-y-3">
                          {(card.items ?? []).map((item) => {
                            const key = `${card.cardKey}/${item.itemKey}`;
                            const itemDraftValue = itemDrafts[key] ?? itemDraft(item);
                            const updateDraftValue = updateDrafts[key] ?? updateDraft(item.update);
                            return (
                              <div key={key} className="rounded border border-slate-200 p-2">
                                <div className="mb-2 flex items-center justify-between">
                                  <span className="font-mono text-xs text-slate-500">
                                    {item.itemKey}
                                  </span>
                                  <div className="flex gap-2">
                                    <button
                                      type="button"
                                      className="rounded border border-slate-300 px-2 py-1 text-xs"
                                      disabled={busy}
                                      onClick={() =>
                                        void run(
                                          () =>
                                            service.upsertItem(
                                              catalog.catalogKey,
                                              card.cardKey,
                                              item.itemKey,
                                              itemBody(itemDraftValue),
                                            ),
                                          reload,
                                        )
                                      }
                                    >
                                      {t.save}
                                    </button>
                                    <button
                                      type="button"
                                      className="rounded border border-red-300 px-2 py-1 text-xs text-red-600"
                                      disabled={busy}
                                      onClick={() => {
                                        if (!window.confirm(t.confirmDelete)) return;
                                        void run(
                                          () =>
                                            service.deleteItem(
                                              catalog.catalogKey,
                                              card.cardKey,
                                              item.itemKey,
                                            ),
                                          reload,
                                        );
                                      }}
                                    >
                                      {t.delete}
                                    </button>
                                  </div>
                                </div>
                                <div className="grid grid-cols-2 gap-2 md:grid-cols-4">
                                  <Field label={t.platform}>
                                    <select
                                      className="w-full rounded border border-slate-300 px-2 py-1"
                                      value={itemDraftValue.platform}
                                      onChange={(event) =>
                                        patchItem(key, { platform: event.target.value })
                                      }
                                    >
                                      {PLATFORMS.map((platform) => (
                                        <option key={platform} value={platform}>
                                          {platform}
                                        </option>
                                      ))}
                                    </select>
                                  </Field>
                                  <Field label={t.architecture}>
                                    <select
                                      className="w-full rounded border border-slate-300 px-2 py-1"
                                      value={itemDraftValue.architecture}
                                      onChange={(event) =>
                                        patchItem(key, { architecture: event.target.value })
                                      }
                                    >
                                      {ARCHITECTURES.map((architecture) => (
                                        <option key={architecture} value={architecture}>
                                          {architecture}
                                        </option>
                                      ))}
                                    </select>
                                  </Field>
                                  <Field label={t.label}>
                                    <input
                                      className="w-full rounded border border-slate-300 px-2 py-1"
                                      value={itemDraftValue.label}
                                      onChange={(event) =>
                                        patchItem(key, { label: event.target.value })
                                      }
                                    />
                                  </Field>
                                  <Field label={t.version}>
                                    <input
                                      className="w-full rounded border border-slate-300 px-2 py-1"
                                      value={itemDraftValue.version}
                                      onChange={(event) =>
                                        patchItem(key, { version: event.target.value })
                                      }
                                    />
                                  </Field>
                                  <Field label={t.fileName}>
                                    <input
                                      className="w-full rounded border border-slate-300 px-2 py-1"
                                      value={itemDraftValue.fileName}
                                      onChange={(event) =>
                                        patchItem(key, { fileName: event.target.value })
                                      }
                                    />
                                  </Field>
                                  <Field label={t.downloadUrl}>
                                    <input
                                      className="w-full rounded border border-slate-300 px-2 py-1"
                                      value={itemDraftValue.href}
                                      onChange={(event) =>
                                        patchItem(key, { href: event.target.value })
                                      }
                                    />
                                  </Field>
                                  <Field label={t.order}>
                                    <input
                                      className="w-full rounded border border-slate-300 px-2 py-1"
                                      value={itemDraftValue.sizeBytes}
                                      onChange={(event) =>
                                        patchItem(key, { sizeBytes: event.target.value })
                                      }
                                    />
                                  </Field>
                                  <div className="flex items-end gap-3 pb-1">
                                    <label className="inline-flex items-center gap-1 text-xs">
                                      <input
                                        type="checkbox"
                                        checked={itemDraftValue.primaryAction}
                                        onChange={(event) =>
                                          patchItem(key, {
                                            primaryAction: event.target.checked,
                                          })
                                        }
                                      />
                                      {t.primary}
                                    </label>
                                    <label className="inline-flex items-center gap-1 text-xs">
                                      <input
                                        type="checkbox"
                                        checked={itemDraftValue.disabled}
                                        onChange={(event) =>
                                          patchItem(key, { disabled: event.target.checked })
                                        }
                                      />
                                      {t.disabled}
                                    </label>
                                  </div>
                                </div>

                                <div className="mt-2 rounded bg-slate-50 p-2">
                                  <div className="mb-2 text-xs font-semibold uppercase text-slate-500">
                                    {t.updateConfig}
                                  </div>
                                  <div className="grid grid-cols-2 gap-2 md:grid-cols-4">
                                    <Field label={t.provider}>
                                      <select
                                        className="w-full rounded border border-slate-300 px-2 py-1"
                                        value={updateDraftValue.provider}
                                        onChange={(event) =>
                                          patchUpdate(key, { provider: event.target.value })
                                        }
                                      >
                                        {PROVIDERS.map((provider) => (
                                          <option key={provider} value={provider}>
                                            {provider}
                                          </option>
                                        ))}
                                      </select>
                                    </Field>
                                    <Field label={t.feedUrl}>
                                      <input
                                        className="w-full rounded border border-slate-300 px-2 py-1"
                                        value={updateDraftValue.feedUrl}
                                        onChange={(event) =>
                                          patchUpdate(key, { feedUrl: event.target.value })
                                        }
                                      />
                                    </Field>
                                    <Field label={t.channel}>
                                      <select
                                        className="w-full rounded border border-slate-300 px-2 py-1"
                                        value={updateDraftValue.channelKey}
                                        onChange={(event) =>
                                          patchUpdate(key, { channelKey: event.target.value })
                                        }
                                      >
                                        {CHANNELS.map((channel) => (
                                          <option key={channel} value={channel}>
                                            {channel}
                                          </option>
                                        ))}
                                      </select>
                                    </Field>
                                    <Field label={t.minVersion}>
                                      <input
                                        className="w-full rounded border border-slate-300 px-2 py-1"
                                        value={updateDraftValue.minimumSupportedVersion}
                                        onChange={(event) =>
                                          patchUpdate(key, {
                                            minimumSupportedVersion: event.target.value,
                                          })
                                        }
                                      />
                                    </Field>
                                    <Field label={t.interval}>
                                      <input
                                        className="w-full rounded border border-slate-300 px-2 py-1"
                                        value={updateDraftValue.checkIntervalMinutes}
                                        onChange={(event) =>
                                          patchUpdate(key, {
                                            checkIntervalMinutes: event.target.value,
                                          })
                                        }
                                      />
                                    </Field>
                                    <Field label={t.rollout}>
                                      <input
                                        className="w-full rounded border border-slate-300 px-2 py-1"
                                        value={updateDraftValue.rolloutPercentage}
                                        onChange={(event) =>
                                          patchUpdate(key, { rolloutPercentage: event.target.value })
                                        }
                                      />
                                    </Field>
                                    <div className="flex items-end gap-3 pb-1">
                                      <label className="inline-flex items-center gap-1 text-xs">
                                        <input
                                          type="checkbox"
                                          checked={updateDraftValue.allowPrerelease}
                                          onChange={(event) =>
                                            patchUpdate(key, {
                                              allowPrerelease: event.target.checked,
                                            })
                                          }
                                        />
                                        {t.allowPrerelease}
                                      </label>
                                      <label className="inline-flex items-center gap-1 text-xs">
                                        <input
                                          type="checkbox"
                                          checked={updateDraftValue.allowDowngrade}
                                          onChange={(event) =>
                                            patchUpdate(key, {
                                              allowDowngrade: event.target.checked,
                                            })
                                          }
                                        />
                                        {t.allowDowngrade}
                                      </label>
                                    </div>
                                  </div>
                                  <button
                                    type="button"
                                    className="mt-2 rounded bg-slate-900 px-3 py-1 text-xs text-white"
                                    disabled={busy}
                                    onClick={() =>
                                      void run(
                                        () =>
                                          service.upsertUpdate(
                                            catalog.catalogKey,
                                            card.cardKey,
                                            item.itemKey,
                                            updateBody(updateDraftValue),
                                          ),
                                        reload,
                                      )
                                    }
                                  >
                                    {t.saveUpdate}
                                  </button>
                                </div>
                              </div>
                            );
                          })}
                        </div>

                        <div className="mt-2 flex gap-1">
                          <input
                            className="w-40 rounded border border-slate-300 px-2 py-1 text-xs"
                            placeholder={t.itemKey}
                            value={newItemKeys[card.cardKey] ?? ""}
                            onChange={(event) =>
                              setNewItemKeys((current) => ({
                                ...current,
                                [card.cardKey]: event.target.value,
                              }))
                            }
                          />
                          <button
                            type="button"
                            className="rounded border border-slate-300 px-2 py-1 text-xs"
                            disabled={busy || (newItemKeys[card.cardKey] ?? "").trim() === ""}
                            onClick={() => {
                              const itemKey = (newItemKeys[card.cardKey] ?? "").trim();
                              void run(
                                () =>
                                  service.upsertItem(catalog.catalogKey, card.cardKey, itemKey, {
                                    platform: "windows",
                                    architecture: "x64",
                                    label: itemKey,
                                  }),
                                async () => {
                                  setNewItemKeys((current) => ({ ...current, [card.cardKey]: "" }));
                                  await reload();
                                },
                              );
                            }}
                          >
                            {t.addItem}
                          </button>
                        </div>
                      </div>
                    </div>
                  );
                })}
              </div>

              <div className="flex gap-1">
                <input
                  className="w-40 rounded border border-slate-300 px-2 py-1 text-xs"
                  placeholder={t.cardKey}
                  value={newCardKey}
                  onChange={(event) => setNewCardKey(event.target.value)}
                />
                <button
                  type="button"
                  className="rounded border border-slate-300 px-2 py-1 text-xs"
                  disabled={busy || newCardKey.trim() === ""}
                  onClick={() => {
                    const cardKey = newCardKey.trim();
                    void run(
                      () =>
                        service.upsertCard(catalog.catalogKey, cardKey, {
                          title: cardKey,
                          description: "",
                        }),
                      async () => {
                        setNewCardKey("");
                        await reload();
                      },
                    );
                  }}
                >
                  {t.addCard}
                </button>
              </div>
            </>
          )}
        </section>
      </div>
    </div>
  );
}

function Field({ children, label }: { children: React.ReactNode; label: string }) {
  return (
    <label className="block text-xs">
      <span className="mb-1 block text-slate-500">{label}</span>
      {children}
    </label>
  );
}
