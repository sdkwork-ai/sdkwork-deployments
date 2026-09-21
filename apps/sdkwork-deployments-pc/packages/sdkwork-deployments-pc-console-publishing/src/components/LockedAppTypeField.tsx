/**
 * LockedAppTypeField — 发布阶段的「应用类型」只读展示（v6）。
 *
 * 为什么是只读而不是可编辑网格：应用类型（`deploy_app.app_kind`）在
 * `CreateAppDialog` 里已经确定并落库，它决定这张应用被部署到哪个平台、以及
 * 截图尺寸档位等下游行为。发布阶段再允许改动，等于让用户用发布动作**改写
 * 应用身份** —— 那是一次意外的类型迁移，不是一次发布。所以这里只展示，
 * 不提供任何写入路径。
 *
 * 组件刻意不复用 `DeployAppTypeGrid`：网格的语义是「从 N 个里选 1 个」
 * （radiogroup + onChange），而这里恰恰相反 —— 答案是给定的。复用会让
 * 一个只读区看起来可点击，还会把「9 张卡全部置灰」这种错误的可供性
 * 摆到用户面前。
 *
 * 两种降级路径（都不引入写入能力）：
 *   - `appKind` 未被发布器识别（契约演进出新成员）→ 展示原文枚举 + 提示重建；
 *   - `appKind` 对应多张卡（`SPA_WEB` ⊃ h5 / pc-web）→ 让用户在等价卡片里
 *     指明**表面**，因为契约无法区分、而表面根决定 metadata.surface。
 */
import type { PublishingTranslator } from "../i18n.ts";
import {
  DEPLOY_APP_TYPE_CARDS,
  type DeployAppKind,
  type DeployAppTypeCard,
} from "../service/deploy-app-publishing.ts";
import { DeployAppTypeIcon } from "./DeployAppTypeIcon.tsx";
import css from "./create-deploy-app.module.css";

export interface LockedAppTypeFieldProps {
  /**
   * 该应用对应的候选卡片（来自 `cardsOfAppKind`）。
   * 空数组 = 契约里没有对应的卡片，展示原文枚举并提示重建。
   */
  readonly cards: readonly DeployAppTypeCard[]
  /** `deploy_app.app_kind` 原文，用于未识别时如实展示。 */
  readonly appKind: DeployAppKind | undefined
  /** 当前表面已确定时选中的卡片 id（多候选卡片的用户选择）。 */
  readonly cardId: string | undefined
  /** 多候选时才被调用：指明本次要发布的表面。 */
  readonly onSelectCard?: ((cardId: string) => void) | undefined
  readonly t: PublishingTranslator
}

export function LockedAppTypeField({
  cards,
  appKind,
  cardId,
  onSelectCard,
  t,
}: LockedAppTypeFieldProps) {
  const single = cards.length === 1 ? cards[0] : undefined;
  const ambiguous = cards.length > 1;
  // 只读取值优先级：用户在多候选里选过的 > 唯一候选。都缺时展示原文。
  const shown = cards.find((card) => card.id === cardId) ?? single;

  return (
    <div className={css.field}>
      <span className={css.fieldLabel}>{t("appTypeLocked")}</span>
      {shown !== undefined ? (
        <>
          {/* 只读展示用 `div` 而非 `button`：没有可聚焦的交互，键盘用户不会
              停在一个点了没反应的控件上。 */}
          <div className={css.typeLockedTile}>
            <span className={css.typeCardIcon}>
              <DeployAppTypeIcon iconKey={shown.iconKey} size={22} />
            </span>
            <span className={css.typeLockedText}>
              <strong>{t(shown.labelKey)}</strong>
              <small>{t(shown.hintKey)}</small>
            </span>
            <span className={css.typeLockedBadge}>{t("typeLockedBadge")}</span>
          </div>
          <span className={css.fieldHint}>
            {ambiguous ? t("appTypeLockedAmbiguous") : t("appTypeLockedHint")}
          </span>
        </>
      ) : (
        <>
          <div className={css.typeLockedTile} data-unknown="true">
            <span className={css.typeLockedText}>
              <strong>{appKind ?? t("userUnknown")}</strong>
              <small>{t("appTypeLockedUnknown")}</small>
            </span>
          </div>
        </>
      )}
      {/* 多候选（SPA_WEB ⊃ H5 / PC 网页）：契约分不清表面，只有目录能分清。
          这仍然不是「修改应用类型」—— 两张卡的 appKind 相同，改的只是
          metadata.surface，即本次发布到哪个表面根。 */}
      {ambiguous && onSelectCard !== undefined && (
        <div className={css.typeLockedChoices} role="radiogroup" aria-label={t("appTypeLocked")}>
          {cards.map((card) => (
            <button
              key={card.id}
              type="button"
              role="radio"
              aria-checked={shown?.id === card.id}
              className={css.typeLockedChoice}
              data-selected={shown?.id === card.id}
              onClick={() => { onSelectCard(card.id) }}
            >
              <span className={css.typeCardIcon}>
                <DeployAppTypeIcon iconKey={card.iconKey} size={18} />
              </span>
              <span>{t(card.labelKey)}</span>
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

/** 卡片 id 全集，供调用方核对反查结果（测试与宿主断言用）。 */
export const LOCKED_APP_TYPE_CARD_IDS: readonly string[] = DEPLOY_APP_TYPE_CARDS.map((card) => card.id);
