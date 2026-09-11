/**
 * Application-type grid (发布对话框 v4 第 1 步).
 *
 * Icon + name cards per the product requirement: the dialog opens on the
 * type grid. Framework / architecture selection (native, Flutter, React
 * Native, …) deliberately lives in the *next* step, not inline here, so every
 * card stays one tap. Cards whose surface was auto-detected in the project
 * directory carry a "检测到" badge.
 *
 * v4 需求 2：项目符合 sdkwork 规范时，网格必须显式给出「支持 / 不支持」——
 * 项目 `apps/` 下确实存在对应表面根的卡片可选，缺失的卡片置灰不可点击并标出
 * 缺失的规范目录；非 sdkwork 项目（或表面清单不可读）时不做任何限制，全部
 * 卡片可选，由用户自行选择。
 */
import type { PublishingTranslator } from "../i18n.ts";
import {
  DEPLOY_APP_TYPE_CARDS,
  type DeployAppTypeAvailability,
  type DeployAppTypeCard,
} from "../service/deploy-app-publishing.ts";
import { DeployAppTypeIcon } from "./DeployAppTypeIcon.tsx";
import css from "./create-deploy-app.module.css";

export interface DeployAppTypeGridProps {
  /** Selected primary card id. */
  readonly cardId: string | undefined
  readonly onChange: (cardId: string) => void
  /** Card surfaced by directory auto-detection, if any. */
  readonly suggestedCardId?: string | undefined
  /**
   * v4 per-card availability. Omitted (or a card missing from the list) means
   * "selectable" — the grid never invents a restriction.
   */
  readonly availability?: readonly DeployAppTypeAvailability[] | undefined
  /** True when availability was derived from a sdkwork project's apps/ listing. */
  readonly gated?: boolean | undefined
  readonly t: PublishingTranslator
}

export function DeployAppTypeGrid({
  cardId,
  onChange,
  suggestedCardId,
  availability,
  gated = false,
  t,
}: DeployAppTypeGridProps) {
  const available = new Map((availability ?? []).map((entry) => [entry.cardId, entry]));
  const selectCard = (card: DeployAppTypeCard) => {
    onChange(card.id);
  };

  return (
    <div className={css.field}>
      <span className={css.fieldLabel}>{t("appType")}</span>
      <span className={css.fieldHint}>{gated ? t("appTypeGridGatedHint") : t("appTypeGridHint")}</span>
      <div className={css.typeCardGrid} role="radiogroup" aria-label={t("appType")}>
        {DEPLOY_APP_TYPE_CARDS.map((card) => {
          const selected = cardId === card.id;
          const suggested = suggestedCardId === card.id;
          const entry = available.get(card.id);
          const supported = entry?.supported !== false;
          return (
            <button
              key={card.id}
              type="button"
              role="radio"
              aria-checked={selected}
              aria-disabled={!supported}
              disabled={!supported}
              data-selected={selected}
              data-supported={supported}
              className={css.typeCardTile}
              onClick={() => { selectCard(card); }}
            >
              <span className={css.typeCardIcon}><DeployAppTypeIcon iconKey={card.iconKey} /></span>
              <span className={css.typeCardName}>{t(card.labelKey)}</span>
              <span className={css.typeCardHint}>{t(card.hintKey)}</span>
              {suggested && supported && <span className={css.typeCardBadge}>{t("typeSuggested")}</span>}
              {gated && (
                <span className={css.typeCardStatus} data-supported={supported}>
                  {supported ? t("typeSupported") : t("typeUnsupported")}
                </span>
              )}
              {gated && !supported && entry?.requiredDirectory !== undefined && (
                <span className={css.typeCardBlocked}>
                  {t("typeUnsupportedRequires", { directory: entry.requiredDirectory })}
                </span>
              )}
            </button>
          );
        })}
      </div>
    </div>
  );
}
