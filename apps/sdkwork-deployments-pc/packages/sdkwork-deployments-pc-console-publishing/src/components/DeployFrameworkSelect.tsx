/**
 * Framework & architecture select (v3.2: 目录步骤内、路径下方).
 *
 * 参照 Vercel/Railway 导入流程：根据所选目录自动检测框架（目录标记如
 * `unpackage`/`.dart_tool`/`src-tauri`/`.nuxt`），命中项带「检测到」徽标；
 * 检测不到时回退卡片默认框架，用户始终可手动覆盖。每个选项预览其默认
 * 构建产物目录。
 *
 * v4 需求 4：所选项目必须符合项目自身的应用架构 —— 当项目表面目录已经给出
 * 决定性架构信号（例如 `unpackage` 表明 uni-app）时，自带标识目录却在该项目
 * 中缺席的框架会被判为架构冲突并置灰不可点击；无标识目录的兜底框架不受影响。
 */
import type { PublishingTranslator } from "../i18n.ts";
import {
  type DeployFrameworkOption,
} from "../service/deploy-app-publishing.ts";
import css from "./create-deploy-app.module.css";

export interface DeployFrameworkSelectProps {
  /** Framework options of the selected card. */
  readonly frameworks: readonly DeployFrameworkOption[]
  /** Selected framework id. */
  readonly frameworkId: string | undefined
  /** Framework id auto-detected from the directory listing, if any. */
  readonly autoDetectedId?: string | undefined
  /** v4: framework ids whose markers contradict the project's architecture. */
  readonly conflictingIds?: readonly string[] | undefined
  readonly t: PublishingTranslator
  readonly onChange: (frameworkId: string) => void
}

export function DeployFrameworkSelect({
  frameworks,
  frameworkId,
  autoDetectedId,
  conflictingIds,
  t,
  onChange,
}: DeployFrameworkSelectProps) {
  const conflicting = new Set(conflictingIds ?? []);
  return (
    <div className={css.field}>
      <span className={css.fieldLabel}>{t("frameworkLabel")}</span>
      <span className={css.fieldHint}>
        {frameworkStepHint(conflicting.size, t)}
      </span>
      <div className={css.frameworkGrid} role="radiogroup" aria-label={t("frameworkLabel")}>
        {frameworks.map((framework) => {
          const selected = frameworkId === framework.id;
          const autoDetected = autoDetectedId === framework.id;
          const mismatch = conflicting.has(framework.id);
          return (
            <button
              key={framework.id}
              type="button"
              role="radio"
              aria-checked={selected}
              aria-disabled={mismatch}
              disabled={mismatch}
              data-selected={selected}
              data-conflicting={mismatch}
              className={css.frameworkTile}
              onClick={() => { onChange(framework.id); }}
            >
              <span className={css.frameworkName}>{t(framework.labelKey)}</span>
              {framework.buildOutputPath !== undefined && (
                <span className={css.frameworkBuildOutput}>
                  {t("frameworkBuildOutputPrefix")}<code>{framework.buildOutputPath}</code>
                </span>
              )}
              {autoDetected && <span className={css.typeCardBadge}>{t("typeSuggested")}</span>}
              {mismatch && <span className={css.frameworkMismatch}>{t("fwArchitectureMismatch")}</span>}
            </button>
          );
        })}
      </div>
    </div>
  );
}

/** 目录信号约束了可选架构时，提示文案随之收紧。 */
function frameworkStepHint(mismatchCount: number, t: PublishingTranslator): string {
  return mismatchCount > 0 ? t("frameworkStepConstrainedHint") : t("frameworkStepHint");
}
