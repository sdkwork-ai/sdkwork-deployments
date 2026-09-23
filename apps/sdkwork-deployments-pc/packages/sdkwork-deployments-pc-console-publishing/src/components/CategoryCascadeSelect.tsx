/**
 * Multi-level category cascade selector (需求 3, v3 交互修订; v3.3 遮挡修复).
 *
 * 单个下拉选择框完成多级级联（参照 Ant Design Cascader 列式交互）：
 * 触发器展示当前分类路径；展开后按列逐级下钻（一级 → 二级 → 三级），
 * 点选任意层级即完成选择（三级可选），选中叶子节点后自动收起。
 * 最终选择以 `{ id, path }` 落入 `deploy_app.metadata.category`，path 为
 * 翻译后的可读标签链。
 *
 * v3.3：面板经 React portal 渲染到 `document.body` 并用 fixed 定位 ——
 * 对话框滚动容器（overflow-y: auto）与对话框圆角容器（overflow: hidden）
 * 不再裁剪/遮挡面板；坐标按触发器 getBoundingClientRect 实时计算，
 * 视口下方空间不足时自动向上翻转，滚动/缩放时跟随重算。
 */
import { useEffect, useMemo, useRef, useState, type CSSProperties } from "react";
import { createPortal } from "react-dom";
import type { AppKind } from "@sdkwork/deployments-pc-console-core/sdk";
import type { PublishingTranslator } from "../i18n.ts";
import { categoriesForAppKind, findCategoryNode, type DeployAppCategoryNode } from "../service/app-categories.ts";
import type { DeployAppCategorySelection } from "../service/deploy-app-publishing.ts";
import css from "./create-deploy-app.module.css";

export interface CategoryCascadeSelectProps {
  readonly appKind: AppKind | undefined
  readonly value: DeployAppCategorySelection | undefined
  readonly onChange: (selection: DeployAppCategorySelection | undefined) => void
  readonly t: PublishingTranslator
  /** 面板被 portal 到 body 后无法继承对话框主题，需显式下发（缺省浅色）。 */
  readonly theme?: ("light" | "dark") | undefined
}

/** Walk the taxonomy and return the ancestor chain of one node (inclusive). */
function ancestorsOf(
  node: DeployAppCategoryNode,
  tree: readonly DeployAppCategoryNode[],
): readonly DeployAppCategoryNode[] {
  const chain: DeployAppCategoryNode[] = [];
  const walk = (candidates: readonly DeployAppCategoryNode[], target: DeployAppCategoryNode): boolean => {
    for (const candidate of candidates) {
      chain.push(candidate);
      if (candidate.id === target.id) return true;
      if (candidate.children && walk(candidate.children, target)) return true;
      chain.pop();
    }
    return false;
  };
  walk(tree, node);
  return chain;
}

/** 面板单列最大高度（与 CSS .cascaderColumn max-height 保持一致）+ 内边距余量。 */
const PANEL_ESTIMATED_HEIGHT = 260;
const PANEL_GAP_PX = 4;

export function CategoryCascadeSelect({ appKind, value, onChange, t, theme = "light" }: CategoryCascadeSelectProps) {
  const tree = useMemo(() => categoriesForAppKind(appKind), [appKind]);
  const [open, setOpen] = useState(false);
  // 浏览路径：决定面板展示到第几列；与已选值保持同步。
  const [activePath, setActivePath] = useState<readonly string[]>(() => value?.path.map((entry) => entry.id) ?? []);
  const [panelStyle, setPanelStyle] = useState<CSSProperties>({});
  const rootRef = useRef<HTMLDivElement | null>(null);
  const panelRef = useRef<HTMLDivElement | null>(null);

  const updatePanelPosition = () => {
    const trigger = rootRef.current;
    if (trigger === null) return;
    const rect = trigger.getBoundingClientRect();
    const spaceBelow = window.innerHeight - rect.bottom;
    // 下方放不下且上方空间更大时向上翻转（bottom 锚定触发器顶部）。
    const openUp = spaceBelow < PANEL_ESTIMATED_HEIGHT && rect.top > spaceBelow;
    setPanelStyle(
      openUp
        ? { left: rect.left, bottom: window.innerHeight - rect.top + PANEL_GAP_PX, width: rect.width }
        : { left: rect.left, top: rect.bottom + PANEL_GAP_PX, width: rect.width },
    );
  };

  // 展开期间：点击组件外（含面板外）或按 Escape 收起；滚动/缩放跟随重算位置。
  useEffect(() => {
    if (!open) return;
    const isOutside = (target: Node): boolean =>
      (rootRef.current === null || !rootRef.current.contains(target))
      && (panelRef.current === null || !panelRef.current.contains(target));
    const onPointerDown = (event: MouseEvent) => {
      if (isOutside(event.target as Node)) setOpen(false);
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") setOpen(false);
    };
    updatePanelPosition();
    document.addEventListener("mousedown", onPointerDown);
    document.addEventListener("keydown", onKeyDown);
    window.addEventListener("resize", updatePanelPosition);
    // capture: 捕获对话框内部滚动容器（.body）的滚动事件。
    document.addEventListener("scroll", updatePanelPosition, true);
    return () => {
      document.removeEventListener("mousedown", onPointerDown);
      document.removeEventListener("keydown", onKeyDown);
      window.removeEventListener("resize", updatePanelPosition);
      document.removeEventListener("scroll", updatePanelPosition, true);
    };
  }, [open]);

  // 外部值变化（例如切换应用类型后清空）时同步浏览路径。
  useEffect(() => {
    setActivePath(value?.path.map((entry) => entry.id) ?? []);
  }, [value]);

  const selectedIds = useMemo(() => new Set(value?.path.map((entry) => entry.id) ?? []), [value]);

  // 面板列：第 0 列为一级分类，其后每列为当前浏览路径节点的子级。
  // 分类树是 readonly；拷贝为可变列数组。
  const columns = useMemo(() => {
    const result: DeployAppCategoryNode[][] = [[...tree]];
    for (const id of activePath) {
      const parent = findCategoryNode(id, tree);
      const children = parent?.children ?? [];
      if (children.length === 0) break;
      // 分类树的 children 是 readonly；拷贝一份以构建可变列数组。
      result.push([...children]);
    }
    return result;
  }, [activePath, tree]);

  const pick = (node: DeployAppCategoryNode) => {
    const pathNodes = ancestorsOf(node, tree);
    setActivePath(pathNodes.map((entry) => entry.id));
    onChange({
      id: node.id,
      path: pathNodes.map((entry) => ({ id: entry.id, label: t(entry.labelKey) })),
    });
    // 叶子节点（无下级）选择后收起；有下级则继续展开下一列。
    if (node.children === undefined || node.children.length === 0) setOpen(false);
  };

  const clear = (event: React.MouseEvent) => {
    event.stopPropagation();
    setActivePath([]);
    onChange(undefined);
    setOpen(false);
  };

  return (
    <div className={css.cascader} ref={rootRef}>
      <button
        type="button"
        className={css.cascaderTrigger}
        aria-haspopup="listbox"
        aria-expanded={open}
        onClick={() => { setOpen((current) => !current) }}
      >
        <span className={css.cascaderValue} data-empty={!value}>
          {value !== undefined && value.path.length > 0
            ? value.path.map((entry) => entry.label).join(" / ")
            : t("categoryTriggerPlaceholder")}
        </span>
        {value !== undefined && (
          <span
            className={css.cascaderClear}
            role="button"
            aria-label={t("categoryClear")}
            title={t("categoryClear")}
            onClick={clear}
          >
            ×
          </span>
        )}
        <span className={css.cascaderArrow} data-open={open} aria-hidden="true">▾</span>
      </button>

      {open && createPortal(
        <div className={css.cascaderPanel} data-theme={theme} style={panelStyle} ref={panelRef}>
          {tree.length === 0 ? (
            <div className={css.cascaderEmpty}>{t("categoryNone")}</div>
          ) : (
            columns.map((options, index) => (
              <ul
                key={index}
                className={css.cascaderColumn}
                role="listbox"
                aria-label={t(index === 0 ? "categoryLevel1" : index === 1 ? "categoryLevel2" : "categoryLevel3")}
              >
                {options.map((node) => {
                  const hasChildren = (node.children?.length ?? 0) > 0;
                  return (
                    <li key={node.id}>
                      <button
                        type="button"
                        role="option"
                        aria-selected={selectedIds.has(node.id)}
                        className={css.cascaderOption}
                        data-selected={selectedIds.has(node.id)}
                        data-branch={hasChildren}
                        onClick={() => { pick(node) }}
                      >
                        <span className={css.cascaderOptionLabel}>{t(node.labelKey)}</span>
                        {hasChildren && <span className={css.cascaderMore} aria-hidden="true">›</span>}
                      </button>
                    </li>
                  );
                })}
              </ul>
            ))
          )}
        </div>,
        document.body,
      )}
    </div>
  );
}
