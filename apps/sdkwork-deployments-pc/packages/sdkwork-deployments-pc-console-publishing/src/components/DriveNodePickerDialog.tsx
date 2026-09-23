/**
 * DriveNodePickerDialog — 浏览 sdkwork-drive 网盘的目录树，选一个目录或文件。
 *
 * 为什么不是 `SandboxExplorerView`
 * --------------------------------
 * Drive 有**两个互不相通的数据面**，名字都叫「文件」但完全不是一回事：
 *
 *   1. **网盘节点面**（本组件）—— `drive/spaces` + `drive/spaces/{id}/nodes`，
 *      浏览租户的 personal / team / app_upload 等空间里的文件夹与文件。这是
 *      操作员认知里的「网盘」。
 *   2. **沙箱卷面** —— `drive/sandboxes` + `sandboxEntries`，浏览**服务器本地
 *      文件系统**（如 `/opt/deploy` 的克隆树）。`@sdkwork/drive-pc-sandbox-explorer`
 *      的 `SandboxExplorerView` 走的是这一面，它的 `SandboxExplorerPort` 契约
 *      完全建立在 `sandboxId + logicalPath` 上，没有 space/node 概念。
 *
 * 「上传代码」要复用网盘里已经存在的产物，所以走第 1 面。把
 * `SandboxExplorerView` 套在网盘上需要伪造一个 sandboxId 并把 folder id 伪装成
 * logicalPath —— 那是把两套语义硬粘在一起，任何一侧改契约都会静默错位。因此这里
 * 实现一个只读的网盘浏览器：职责单一、只读、零写入。
 *
 * 与 `UploadSourceDialog` 的分工
 * ------------------------------
 * 本组件只负责「选出 nodeId」；拿到 nodeId 之后的字节拉取、打包、上传、制品登记
 * 全在 `UploadSourceDialog` 既有的 publisher 链路里。所以这里不 import 任何上传
 * 逻辑，双击文件与点「选择」是等价动作。
 *
 * 两个易踩的点：
 * - **空间分页是 cursor 制**，不是 offset；`pageInfo.nextCursor` 是唯一的续页
 *   依据，没有 cursor 就必须停下（否则无限循环）。
 * - **`parentNodeId` 省略 = 列空间根**；传空串会被服务端当成一个不存在的 id。
 *   所以根层必须整个字段省略，不能传 `""`。
 */
import type { SdkworkDriveAppClient } from "@sdkwork/deployments-pc-console-core/sdk";
import type { DeploymentsLocale } from "@sdkwork/deployments-pc-commons";
import { useEffect, useMemo, useRef, useState } from "react";
import { publishingTranslator } from "../i18n.ts";
import css from "./create-deploy-app.module.css";

/** 选中的 Drive 节点，已归一化成上传链路需要的形状。 */
export interface DriveNodeSelection {
  readonly nodeId: string
  readonly spaceId: string
  readonly spaceName: string
  readonly nodeName: string
  /** 目录时为 `folder`，选中文件时为 `file`。 */
  readonly nodeKind: "file" | "folder"
  /** 文件才有；目录为 `undefined`。 */
  readonly contentType?: string | undefined
  /** 文件才有；目录为 `0`。 */
  readonly contentLength: number
  readonly updatedAt: string
  /** 空间名 / 逐级目录名，给调用方做展示。 */
  readonly displayPath: string
}

export interface DriveNodePickerDialogProps {
  readonly driveClient: SdkworkDriveAppClient
  readonly locale: DeploymentsLocale
  /** 关掉面板（右上角 ×、Esc、点遮罩）。 */
  readonly onClose: () => void
  readonly onSelected: (selection: DriveNodeSelection) => void
  readonly theme?: ("light" | "dark") | undefined
}

/** 一个面包屑层级：根（spaceId）或某个文件夹。 */
interface PickerCrumb {
  readonly nodeId: string | undefined
  readonly label: string
}

interface PickerEntry {
  readonly nodeId: string
  readonly nodeName: string
  readonly nodeType: "file" | "folder"
  readonly contentType?: string | undefined
  readonly contentLength: number
  readonly updatedAt: string
}

interface PickerSpace {
  readonly spaceId: string
  readonly spaceName: string
}

/**
 * 一次列表请求的页大小。
 *
 * ⚠️ 两个列表的 `pageSize` **线上类型不一致**，这是契约事实不是笔误：
 * `drive/spaces` 收 `number`，`drive/spaces/{id}/nodes` 收 `string`。统一成
 * 一个常量再各按契约转换，好过在两处各写一个魔法值。
 */
const PAGE_SIZE = 100;
const NODE_PAGE_SIZE = String(PAGE_SIZE);

export function DriveNodePickerDialog({
  driveClient,
  locale,
  onClose,
  onSelected,
  theme = "light",
}: DriveNodePickerDialogProps) {
  const t = useMemo(() => publishingTranslator(locale), [locale]);

  const [spaces, setSpaces] = useState<readonly PickerSpace[]>();
  const [spacesCursor, setSpacesCursor] = useState<string>()
  const [space, setSpace] = useState<PickerSpace>()
  /** 从空间根到当前目录的路径；`nodeId: undefined` 表示空间根。 */
  const [crumbs, setCrumbs] = useState<readonly PickerCrumb[]>([]);
  const [entries, setEntries] = useState<readonly PickerEntry[]>()
  const [entriesCursor, setEntriesCursor] = useState<string>()
  const [loading, setLoading] = useState(false);
  const [loadingMore, setLoadingMore] = useState(false);
  const [picked, setPicked] = useState<PickerEntry>()
  const [error, setError] = useState<string>();

  /** 当前目录的请求序号：快速连点目录时只认最后一次的响应。 */
  const requestRef = useRef(0);
  /** 键盘 Esc 关闭，但加载中不打断在飞的列表请求。 */
  const busy = loading || loadingMore;

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape" && !busy) onClose();
    };
    document.addEventListener("keydown", onKeyDown);
    const previousOverflow = document.body.style.overflow;
    document.body.style.overflow = "hidden";
    return () => {
      document.removeEventListener("keydown", onKeyDown);
      document.body.style.overflow = previousOverflow;
    };
  }, [busy, onClose]);

  // 空间列表：只在打开时取一次，之后靠 cursor 续页。
  useEffect(() => {
    let active = true;
    setLoading(true);
    setError(undefined);
    void driveClient.drive.spaces
      .list({ pageSize: PAGE_SIZE })
      .then((page) => {
        if (!active) return;
        setSpaces(page.items.map((item) => ({ spaceId: item.id, spaceName: item.displayName })));
        setSpacesCursor(page.pageInfo?.nextCursor ?? undefined);
      })
      .catch((cause) => {
        if (!active) return;
        setError(errorText(cause, t("drivePickerLoadFailed")));
      })
      .finally(() => {
        if (active) setLoading(false);
      });
    return () => {
      active = false;
    };
  }, [driveClient, t]);

  const loadDirectory = async (
    nextSpace: PickerSpace,
    parentNodeId: string | undefined,
    nextCrumbs: readonly PickerCrumb[],
  ) => {
    const requestId = ++requestRef.current;
    setLoading(true);
    setError(undefined);
    setPicked(undefined);
    try {
      const page = await driveClient.drive.nodes.list(nextSpace.spaceId, {
        // 空间根必须整个省略 `parentNodeId`；传空串服务端会当成不存在的 id。
        ...(parentNodeId === undefined ? {} : { parentNodeId }),
        pageSize: NODE_PAGE_SIZE,
        sortBy: "name",
        sortOrder: "asc",
      });
      if (requestRef.current !== requestId) return;
      setSpace(nextSpace);
      setCrumbs(nextCrumbs);
      setEntries(page.items.filter((node) => node.nodeType === "file" || node.nodeType === "folder").map((node) => ({
        nodeId: node.id,
        nodeName: node.nodeName,
        nodeType: node.nodeType === "folder" ? "folder" as const : "file" as const,
        contentType: node.contentType ?? undefined,
        contentLength: Number(node.contentLength ?? "0"),
        updatedAt: node.updatedAt,
      })));
      setEntriesCursor(page.pageInfo?.nextCursor ?? undefined);
    } catch (cause) {
      if (requestRef.current !== requestId) return;
      setEntries([]);
      setEntriesCursor(undefined);
      setError(errorText(cause, t("drivePickerLoadFailed")));
    } finally {
      if (requestRef.current === requestId) setLoading(false);
    }
  };

  const loadMoreSpaces = async () => {
    if (spacesCursor === undefined || loadingMore) return;
    setLoadingMore(true);
    setError(undefined);
    try {
      const page = await driveClient.drive.spaces.list({ pageSize: PAGE_SIZE, cursor: spacesCursor });
      setSpaces((current) => [
        ...(current ?? []),
        ...page.items.map((item) => ({ spaceId: item.id, spaceName: item.displayName })),
      ]);
      setSpacesCursor(page.pageInfo?.nextCursor ?? undefined);
    } catch (cause) {
      setError(errorText(cause, t("drivePickerLoadFailed")));
    } finally {
      setLoadingMore(false);
    }
  };

  const loadMoreEntries = async () => {
    if (space === undefined || entriesCursor === undefined || loadingMore) return;
    setLoadingMore(true);
    setError(undefined);
    const requestId = requestRef.current;
    try {
      const parentNodeId = crumbs.at(-1)?.nodeId;
      const page = await driveClient.drive.nodes.list(space.spaceId, {
        ...(parentNodeId === undefined ? {} : { parentNodeId }),
        pageSize: NODE_PAGE_SIZE,
        cursor: entriesCursor,
        sortBy: "name",
        sortOrder: "asc",
      });
      if (requestRef.current !== requestId) return;
      const known = new Set((entries ?? []).map((entry) => entry.nodeId));
      setEntries((current) => [
        ...(current ?? []),
        ...page.items
          .filter((node) => (node.nodeType === "file" || node.nodeType === "folder") && !known.has(node.id))
          .map((node) => ({
            nodeId: node.id,
            nodeName: node.nodeName,
            nodeType: node.nodeType === "folder" ? "folder" as const : "file" as const,
            contentType: node.contentType ?? undefined,
            contentLength: Number(node.contentLength ?? "0"),
            updatedAt: node.updatedAt,
          })),
      ]);
      setEntriesCursor(page.pageInfo?.nextCursor ?? undefined);
    } catch (cause) {
      setError(errorText(cause, t("drivePickerLoadFailed")));
    } finally {
      setLoadingMore(false);
    }
  };

  const openSpace = (nextSpace: PickerSpace) => {
    void loadDirectory(nextSpace, undefined, [{ nodeId: undefined, label: nextSpace.spaceName }]);
  };

  const openFolder = (entry: PickerEntry) => {
    if (space === undefined) return;
    void loadDirectory(space, entry.nodeId, [...crumbs, { nodeId: entry.nodeId, label: entry.nodeName }]);
  };

  const jumpToCrumb = (index: number) => {
    if (space === undefined) return;
    const crumb = crumbs[index];
    if (crumb === undefined) return;
    void loadDirectory(space, crumb.nodeId, crumbs.slice(0, index + 1));
  };

  const drivePath = (entry: PickerEntry): string => {
    const prefix = crumbs.map((crumb) => crumb.label).join(" / ");
    return prefix === "" ? entry.nodeName : `${prefix} / ${entry.nodeName}`;
  };

  /** 选中当前目录本身（面包屑最末一级）。 */
  const selectCurrentDirectory = () => {
    if (space === undefined || crumbs.length === 0) return;
    const current = crumbs.at(-1);
    if (current === undefined) return;
    const parentLabel = crumbs.slice(0, -1).map((crumb) => crumb.label).join(" / ");
    onSelected({
      nodeId: current.nodeId ?? "",
      spaceId: space.spaceId,
      spaceName: space.spaceName,
      nodeName: current.label,
      nodeKind: "folder",
      contentLength: 0,
      updatedAt: "",
      displayPath: parentLabel === "" ? `${space.spaceName} /` : `${space.spaceName} / ${parentLabel}`,
    });
  };

  const confirmPick = () => {
    if (space === undefined || picked === undefined) return;
    onSelected({
      nodeId: picked.nodeId,
      spaceId: space.spaceId,
      spaceName: space.spaceName,
      nodeName: picked.nodeName,
      nodeKind: picked.nodeType,
      contentType: picked.contentType,
      contentLength: picked.contentLength,
      updatedAt: picked.updatedAt,
      displayPath: drivePath(picked),
    });
  };

  const canSelectCurrentDirectory = space !== undefined && crumbs.length > 0 && crumbs.at(-1)?.nodeId !== undefined;

  return (
    <div
      className={css.drawerRoot}
      data-theme={theme}
      role="presentation"
      onMouseDown={(event) => { if (event.target === event.currentTarget && !busy) onClose() }}
    >
      <div
        className={`${css.drawerPanel} ${css.drawerPanelLg}`}
        role="dialog"
        aria-modal="true"
        aria-label={t("drivePickerTitle")}
      >
        <header className={css.header}>
          <div className={css.headerText}>
            <h2>{t("drivePickerTitle")}</h2>
            <p>{t("drivePickerDescription")}</p>
          </div>
          <button type="button" className={css.closeButton} title={t("close")} aria-label={t("close")} onClick={onClose}>
            ×
          </button>
        </header>

        <div className={css.body}>
          {space === undefined ? (
            <>
              <span className={css.fieldLabel}>{t("drivePickerSpaces")}</span>
              {loading && entries === undefined && spaces === undefined && (
                <span className={css.fieldHint}>{t("drivePickerLoading")}</span>
              )}
              {spaces !== undefined && spaces.length === 0 && !loading && (
                <span className={css.fieldHint}>{t("drivePickerNoSpaces")}</span>
              )}
              {spaces !== undefined && spaces.length > 0 && (
                <div className={css.appList}>
                  {spaces.map((candidate) => (
                    <button
                      key={candidate.spaceId}
                      type="button"
                      className={css.appRow}
                      disabled={busy}
                      onClick={() => { openSpace(candidate) }}
                    >
                      <span className={css.appRowMeta}>
                        <strong>{candidate.spaceName}</strong>
                      </span>
                      <span className={css.targetChipBadge}>{t("drivePickerOpenSpace")}</span>
                    </button>
                  ))}
                </div>
              )}
              {spacesCursor !== undefined && (
                <button
                  type="button"
                  className={css.secondaryButton}
                  disabled={loadingMore}
                  onClick={() => { void loadMoreSpaces() }}
                >
                  {loadingMore ? t("drivePickerLoading") : t("drivePickerLoadMore")}
                </button>
              )}
            </>
          ) : (
            <>
              <div className={css.field}>
                <span className={css.fieldLabel}>{t("drivePickerLocation")}</span>
                <div className={css.directoryRow}>
                  {crumbs.map((crumb, index) => (
                    <button
                      key={`${crumb.nodeId ?? "root"}-${index}`}
                      type="button"
                      className={css.secondaryButton}
                      disabled={busy || index === crumbs.length - 1}
                      onClick={() => { jumpToCrumb(index) }}
                    >
                      {crumb.label}
                    </button>
                  ))}
                  <button
                    type="button"
                    className={css.secondaryButton}
                    disabled={busy}
                    onClick={() => { setSpace(undefined); setCrumbs([]); setEntries(undefined); setPicked(undefined) }}
                  >
                    {t("drivePickerSwitchSpace")}
                  </button>
                </div>
              </div>

              <div className={css.field}>
                <span className={css.fieldLabel}>{t("drivePickerContents")}</span>
                {loading && (
                  <span className={css.fieldHint}>{t("drivePickerLoading")}</span>
                )}
                {!loading && entries !== undefined && entries.length === 0 && (
                  <span className={css.fieldHint}>{t("drivePickerEmpty")}</span>
                )}
                {!loading && entries !== undefined && entries.length > 0 && (
                  <div className={css.appList}>
                    {entries.map((entry) => (
                      <button
                        key={entry.nodeId}
                        type="button"
                        className={css.appRow}
                        data-selected={picked?.nodeId === entry.nodeId}
                        disabled={busy}
                        onClick={() => { setPicked(entry); setError(undefined) }}
                        onDoubleClick={() => {
                          if (entry.nodeType === "folder") openFolder(entry);
                          else {
                            setPicked(entry);
                            onSelected({
                              nodeId: entry.nodeId,
                              spaceId: space.spaceId,
                              spaceName: space.spaceName,
                              nodeName: entry.nodeName,
                              nodeKind: "file",
                              contentType: entry.contentType,
                              contentLength: entry.contentLength,
                              updatedAt: entry.updatedAt,
                              displayPath: drivePath(entry),
                            });
                          }
                        }}
                      >
                        <span className={css.appRowMeta}>
                          <strong>{entry.nodeName}</strong>
                          <small>
                            {entry.nodeType === "folder"
                              ? t("drivePickerFolder")
                              : `${formatBytes(entry.contentLength)} · ${formatDate(entry.updatedAt, locale)}`}
                          </small>
                        </span>
                        <span className={css.targetChipBadge}>
                          {picked?.nodeId === entry.nodeId
                            ? t("uploadDriveSelected")
                            : entry.nodeType === "folder" ? t("drivePickerOpen") : t("uploadDriveSelect")}
                        </span>
                      </button>
                    ))}
                  </div>
                )}
                {entriesCursor !== undefined && !loading && (
                  <button
                    type="button"
                    className={css.secondaryButton}
                    disabled={loadingMore}
                    onClick={() => { void loadMoreEntries() }}
                  >
                    {loadingMore ? t("drivePickerLoading") : t("drivePickerLoadMore")}
                  </button>
                )}
              </div>
            </>
          )}
        </div>

        <footer className={css.footer}>
          {error && <div className={css.errorBanner} role="alert">{error}</div>}
          {!error && <div className={css.footerSpacer} />}
          <button type="button" className={css.secondaryButton} disabled={busy} onClick={onClose}>
            {t("cancel")}
          </button>
          {canSelectCurrentDirectory && (
            <button
              type="button"
              className={css.secondaryButton}
              disabled={busy}
              onClick={selectCurrentDirectory}
            >
              {t("drivePickerSelectCurrent")}
            </button>
          )}
          <button
            type="button"
            className={css.primaryButton}
            disabled={busy || picked === undefined}
            onClick={confirmPick}
          >
            {t("drivePickerConfirm")}
          </button>
        </footer>
      </div>
    </div>
  );
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  const units = ["KiB", "MiB", "GiB"];
  let value = bytes / 1024;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return `${value.toFixed(value >= 10 ? 0 : 1)} ${units[unit]}`;
}

function formatDate(value: string, locale: DeploymentsLocale): string {
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? value : date.toLocaleDateString(locale);
}

function errorText(cause: unknown, template: string): string {
  const message = cause instanceof Error && cause.message ? cause.message : String(cause);
  return template.replace("{message}", message);
}
