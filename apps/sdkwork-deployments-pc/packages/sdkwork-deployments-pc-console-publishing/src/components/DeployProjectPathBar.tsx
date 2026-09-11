/**
 * Project path bar (发布对话框 v4 需求 1).
 *
 * 常驻对话框顶部（5 个步骤都在）：显示当前选中的项目路径，并给出该路径的
 * 规范判定与应用表面摘要，让用户在第一步选应用类型之前就知道自己在发布哪个
 * 目录、以及这个目录支持哪些应用类型。
 *
 * 判定文案与 {@link DeployAppTypeGrid} 的「支持 / 不支持」徽标同源，都来自
 * {@link DeployProjectProfile}：只有 sdkwork 项目且 apps/ 表面清单可读时才会
 * 约束应用类型。
 *
 * 非 sdkwork 目录只显示路径本身 —— 规范判定徽标、应用表面摘要与「规范约束」
 * 说明整块不渲染；「不支持的类型为何置灰」只在 sdkwork 项目下出现一次，
 * 不重复描述同一件事。
 */
import type { PublishingTranslator } from "../i18n.ts";
import { APP_SURFACE_LABEL_KEYS } from "../i18n.ts";
import type {
  DeployProjectDetection,
  DeployProjectProfile,
} from "../service/project-detection.ts";
import css from "./create-deploy-app.module.css";

export interface DeployProjectPathBarProps {
  /** 当前选中的项目路径（sourceDirectory 的候选根）。 */
  readonly directory: string | undefined
  /** 宿主正在列举目录。 */
  readonly inspecting: boolean
  /** 目录检测结果（undefined = 尚未/无法检测）。 */
  readonly detection: DeployProjectDetection | undefined
  /** 项目规范画像（驱动类型约束的判定）。 */
  readonly profile: DeployProjectProfile
  readonly t: PublishingTranslator
  /** 更换项目目录（宿主原生选择器）。 */
  readonly onChangeDirectory: () => void
}

export function DeployProjectPathBar({
  directory,
  inspecting,
  detection,
  profile,
  t,
  onChangeDirectory,
}: DeployProjectPathBarProps) {
  const gated = profile.sdkwork && profile.surfaces.length > 0;
  // 规范判定只在 sdkwork 项目里才有信息量：非 sdkwork 目录保持沉默，不再输出
  // 任何规范文案（此前 badge 与 rule 会各自渲染一次「非 sdkwork 规范项目…」，
  // 同一句话在路径栏里上下重复，属于纯噪音）。
  //
  // `unknown` 同样不出徽标：宿主没列举到时它会说「未识别为 sdkwork 规范目录」，
  // 与下方「sdkwork 项目 —— 未读到 apps/ 表面列表」自相矛盾，只留后者。
  const badgeKey = !profile.sdkwork || detection === undefined || detection.conformance === "unknown"
    ? undefined
    : detection.conformance === "conformant"
      ? "detectionConformant"
      : "detectionPartial";
  const badge = badgeKey !== undefined ? t(badgeKey) : undefined;
  const badgeConformance = detection?.conformance ?? "partial";
  const surfaceLabels = profile.surfaces
    .map((surface) => t(APP_SURFACE_LABEL_KEYS[surface]))
    .join(" · ");

  // 只有 sdkwork 项目需要向用户交代「为什么类型被限制 / 为什么没被限制」。
  const rule = !profile.sdkwork
    ? undefined
    : gated
      ? t("projectKindSdkworkGate")
      : t("projectKindSdkworkUnlisted");

  // 非 sdkwork 目录：除「正在检测」外没有任何判定内容 → 整行不渲染，避免留下
  // 一行空白（0 内容的 flex 行仍会撑出间距）。
  const showMeta = inspecting
    || badge !== undefined
    || profile.surfaces.length > 0
    || (profile.sdkwork && detection !== undefined);

  return (
    <div className={css.projectPathBar} data-sdkwork={profile.sdkwork} data-gated={gated}>
      <div className={css.projectPathHead}>
        <span className={css.projectPathLabel}>{t("projectPath")}</span>
        <code className={css.projectPathValue} title={directory ?? ""}>
          {directory !== undefined && directory.trim() !== "" ? directory : t("noDirectory")}
        </code>
        <button type="button" className={css.secondaryButton} onClick={onChangeDirectory}>
          {t("changeDirectory")}
        </button>
      </div>

      {showMeta && (
        <div className={css.projectPathMeta}>
          {inspecting && <span className={css.projectPathInspecting} aria-busy="true">{t("directoryInspecting")}</span>}
          {!inspecting && badge !== undefined && (
            <span className={css.detectBadge} data-conformance={badgeConformance}>{badge}</span>
          )}
          {profile.sdkwork && profile.applicationCode !== undefined && (
            <span className={css.detectCode}>
              {t("detectionApplicationCode")}: <code>{profile.applicationCode}</code>
            </span>
          )}
          {profile.surfaces.length > 0 && (
            <span className={css.projectSurfaceSummary}>
              {t("projectSurfacesDetected", { surfaces: surfaceLabels })}
            </span>
          )}
          {profile.surfaces.length === 0 && profile.sdkwork && detection !== undefined && (
            <span className={css.projectSurfaceSummary}>{t("projectSurfacesNone")}</span>
          )}
        </div>
      )}

      {rule !== undefined && <div className={css.projectPathRule}>{rule}</div>}
    </div>
  );
}
