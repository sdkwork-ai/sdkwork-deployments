/**
 * ⚠️ 未落地（v7）：本文件没有任何生产消费者，只被已 park 的
 * `tests/pending/publish-first-step-v7.spec.ts.pending` 引用（见该目录 README）。
 * 它是 v7 发布对话框的零件，接线前不要把它当成在跑的代码。
 *
 * FieldFeedback — 字段级「必填 / 报错」反馈（v7）。
 *
 * 背景（用户实测反馈）：「点击下一步：请先填写必填项再发布」—— 界面上既没有
 * 哪些字段必填的标记，也没有把错误落到具体那一项。用户看到一句话，不知道
 * 该去改哪里，只能逐字段试。
 *
 * 因此校验从「一个布尔 + 一句概括」改为**字段级**：
 *   - `RequiredTag`   —— 必填字段的标签右侧标出「必填」；
 *   - `FieldError`    —— 该字段下方给出**它自己**的原因（`role="alert"`，
 *                        读屏器在错误出现时立即播报）；
 *   - `data-field-invalid` 由调用方挂在字段容器上，供对话框聚焦第一个出错项。
 *
 * 这里刻意不放任何校验逻辑：判据留在 `CreateDeployAppDialog` 的
 * `validateFields`（单一事实来源），本文件只管把结果画出来。
 */
import type { PublishingTranslator } from "../i18n.ts";
import css from "./create-deploy-app.module.css";

/** 字段级反馈：`error` 为 undefined 表示该项当前没有错误。 */
export interface DeployFieldFeedback {
  readonly error?: string | undefined
}

/** 必填字段标签右侧的「必填」标记。 */
export function RequiredTag({ t }: { readonly t: PublishingTranslator }) {
  return <em className={css.fieldRequired}>{t("fieldRequired")}</em>;
}

/** 字段下方的行内错误行；无错误时不渲染任何节点。 */
export function FieldError({ error }: DeployFieldFeedback) {
  if (error === undefined) return null;
  return (
    <span className={css.fieldError} role="alert">
      {error}
    </span>
  );
}
