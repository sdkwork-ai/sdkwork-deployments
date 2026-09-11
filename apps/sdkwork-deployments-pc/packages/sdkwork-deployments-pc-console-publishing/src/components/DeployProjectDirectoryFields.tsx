/**
 * Project directory fields with sdkwork-specs auto-detection (v3 第 3 步).
 *
 * 双路径模型（sdkwork 应用）：
 *   1. 应用根路径 —— `apps/sdkwork-<code>-<surface>` 表面目录（自动发现，
 *      默认取当前会话/项目目录）；
 *   2. 构建产物相对路径 —— build 后的资源文件目录（按框架给出默认值，
 *      并用宿主目录列举验证是否存在，展示候选 chip）。
 *
 * When the host provides an inspection port, the panel reports layout
 * conformance (APPLICATION_DEPLOY_LAYOUT_SPEC §2 markers), the derived
 * application code, and the detected `apps/sdkwork-<code>-<surface>` roots —
 * highlighting the surface matching the selected application type and
 * auto-deriving its publish source directory.
 *
 * 该面板只在 sdkwork 项目下渲染：非 sdkwork 目录没有规范可言，输出「未识别为
 * sdkwork 规范目录」+ 一列全红的规范标记既无信息量也干扰发布操作。
 */
import type { PublishingTranslator } from "../i18n.ts";
import { APP_SURFACE_LABEL_KEYS } from "../i18n.ts";
import {
  findDetectedSurface,
  type AppSurfaceId,
  type DeployProjectDetection,
} from "../service/project-detection.ts";
import css from "./create-deploy-app.module.css";

export interface DeployProjectDirectoryFieldsProps {
  /** 应用根路径（sourceDirectory）。 */
  readonly directory: string
  /** 构建产物目录（相对应用根）。 */
  readonly buildOutputPath: string
  /** Detection result for the current directory (undefined = not inspectable). */
  readonly detection: DeployProjectDetection | undefined
  readonly inspecting: boolean
  /** Surface of the currently selected application type. */
  readonly selectedSurface: AppSurfaceId | undefined
  /** Source directory auto-derived for the selected surface. */
  readonly matchedSurfacePath: string | undefined
  /**
   * Whether the build-output path was observed under the surface root:
   * true = detected, false = listed but missing, undefined = not listable.
   */
  readonly buildOutputDetected: boolean | undefined
  /** Generic build-output directory names observed in the surface root. */
  readonly buildCandidates: readonly string[]
  readonly t: PublishingTranslator
  readonly onDirectoryChange: (value: string) => void
  readonly onBuildOutputChange: (value: string) => void
  readonly onChangeDirectoryClick: () => void
  readonly onReinspect: () => void
}

export function DeployProjectDirectoryFields({
  directory,
  buildOutputPath,
  detection,
  inspecting,
  selectedSurface,
  matchedSurfacePath,
  buildOutputDetected,
  buildCandidates,
  t,
  onDirectoryChange,
  onBuildOutputChange,
  onChangeDirectoryClick,
  onReinspect,
}: DeployProjectDirectoryFieldsProps) {
  const conformanceKey = detection === undefined
    ? undefined
    : detection.conformance === "conformant"
      ? "detectionConformant"
      : detection.conformance === "partial"
        ? "detectionPartial"
        : "detectionUnknown";
  const matchedSurface = findDetectedSurface(detection, selectedSurface);
  // 规范检测面板只服务 sdkwork 项目：非 sdkwork 目录下它只会输出「未识别为
  // sdkwork 规范目录」「规范目录标记（全红）」这类与发布无关的判定文案。
  const sdkworkProject = detection?.project.isSdkworkProject === true;

  return (
    <>
      <div className={css.field}>
        <span className={css.fieldLabel}>{t("sourceDirectory")}</span>
        <span className={css.fieldHint}>{t("sourceDirectoryHint")} {t("directoryAutoDetected")}</span>
        <div className={css.directoryRow}>
          <input
            className={css.input}
            value={directory}
            data-empty={!directory}
            placeholder={t("noDirectory")}
            onChange={(event) => { onDirectoryChange(event.target.value) }}
          />
          <button type="button" className={css.secondaryButton} onClick={onChangeDirectoryClick}>
            {t("changeDirectory")}
          </button>
          <button
            type="button"
            className={css.secondaryButton}
            disabled={!directory || inspecting}
            onClick={onReinspect}
          >
            {t("reinspect")}
          </button>
        </div>
      </div>

      <div className={css.field}>
        <span className={css.fieldLabel}>{t("buildOutputPath")}</span>
        <span className={css.fieldHint}>{t("buildOutputPathHint")}</span>
        <div className={css.directoryRow}>
          <input
            className={css.input}
            value={buildOutputPath}
            placeholder={t("buildOutputPathPlaceholder")}
            onChange={(event) => { onBuildOutputChange(event.target.value) }}
          />
          {buildOutputDetected === true && <span className={css.detectBadge} data-conformance="conformant">{t("buildOutputDetected")}</span>}
          {buildOutputDetected === false && <span className={css.detectBadge} data-conformance="partial">{t("buildOutputNotDetected")}</span>}
        </div>
        {buildCandidates.length > 0 && (
          <div className={css.buildCandidates}>
            {buildCandidates.map((candidate) => (
              <button
                key={candidate}
                type="button"
                className={css.markerChip}
                data-present={buildOutputPath.trim().replace(/^\.\//, "") === candidate}
                onClick={() => { onBuildOutputChange(candidate) }}
              >
                {candidate}
              </button>
            ))}
          </div>
        )}
      </div>

      {inspecting && <div className={css.detectPanel} aria-busy="true">{t("directoryInspecting")}</div>}

      {!inspecting && detection !== undefined && sdkworkProject && (
        <div className={css.detectPanel} data-conformance={detection.conformance}>
          <div className={css.detectHead}>
            <span className={css.detectBadge} data-conformance={detection.conformance}>
              {conformanceKey !== undefined ? t(conformanceKey) : ""}
            </span>
            {detection.applicationCode !== undefined && (
              <span className={css.detectCode}>
                {t("detectionApplicationCode")}: <code>{detection.applicationCode}</code>
              </span>
            )}
          </div>

          {detection.surfaces.length > 0 && (
            <div className={css.detectSection}>
              <span className={css.detectSectionLabel}>{t("detectionSurfaces")}</span>
              <div className={css.surfaceBadges}>
                {/* 一个 apps/ 表面根可承载多个发布目标（如 -flutter-mobile →
                    Android + iOS，-uniapp → 跨端），因此徽标按根聚合展示。 */}
                {detection.surfaces.map((surface) => (
                  <span
                    key={surface.directory}
                    className={css.surfaceBadge}
                    data-matched={selectedSurface !== undefined && surface.surfaces.includes(selectedSurface)}
                    title={surface.path}
                  >
                    {surface.surfaces.map((id) => t(APP_SURFACE_LABEL_KEYS[id])).join(" · ")}
                  </span>
                ))}
              </div>
            </div>
          )}

          <div className={css.detectSection}>
            <span className={css.detectSectionLabel}>{t("detectionMarkers")}</span>
            <div className={css.markerList}>
              {["apps", "deployments", "etc", "specs", ".sdkwork"].map((marker) => (
                <span key={marker} className={css.markerChip} data-present={detection.presentMarkers.includes(marker)}>
                  {marker}
                </span>
              ))}
            </div>
          </div>

          {matchedSurface !== undefined && matchedSurfacePath !== undefined && (
            <div className={css.autoMatchNote}>{t("directoryAutoMatched")} <code>{matchedSurface.path}</code></div>
          )}
          {matchedSurface === undefined && selectedSurface !== undefined && directory !== "" && (
            <div className={css.autoMatchNote}>{t("directoryNoSurfaceMatch")}</div>
          )}
        </div>
      )}
    </>
  );
}
