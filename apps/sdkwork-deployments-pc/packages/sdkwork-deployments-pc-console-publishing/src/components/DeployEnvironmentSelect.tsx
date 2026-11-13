/**
 * Publish-environment selector (v3.4: 目录步骤内、目录字段之前).
 *
 * Environments are the canonical sdkwork-specs ENVIRONMENT_SPEC set
 * (development/test/staging/demo/production — 开发/测试/预发/演示/线上) plus
 * the standalone|cloud deployment mode; together they compose the canonical
 * profile id `<mode>.<environment>` — 该组合同时决定浏览器类应用的构建产物
 * 子树 dist/<mode>/<envAlias>，因此必须先于产物路径字段确定。
 */
import type { PublishingMessageKey, PublishingTranslator } from "../i18n.ts";
import {
  DEPLOY_DEPLOYMENT_MODES,
  DEPLOY_ENVIRONMENT_IDS,
  deployProfileId,
  type DeployDeploymentMode,
  type DeployEnvironmentId,
} from "../service/project-detection.ts";
import css from "./create-deploy-app.module.css";

export interface DeployEnvironmentSelectProps {
  readonly environment: DeployEnvironmentId | undefined
  readonly deploymentMode: DeployDeploymentMode
  readonly onEnvironmentChange: (environment: DeployEnvironmentId) => void
  readonly onDeploymentModeChange: (mode: DeployDeploymentMode) => void
  /** Host can hide the advanced standalone/cloud switch. */
  readonly showDeploymentMode?: boolean | undefined
  readonly t: PublishingTranslator
}

const ENVIRONMENT_LABEL_KEYS: Record<DeployEnvironmentId, PublishingMessageKey> = {
  development: "envDevelopment",
  test: "envTest",
  staging: "envStaging",
  demo: "envDemo",
  production: "envProduction",
};

export function DeployEnvironmentSelect({
  environment,
  deploymentMode,
  onEnvironmentChange,
  onDeploymentModeChange,
  showDeploymentMode = true,
  t,
}: DeployEnvironmentSelectProps) {
  return (
    <>
      <div className={css.field}>
        <span className={css.fieldLabel}>{t("publishEnvironment")}</span>
        <span className={css.fieldHint}>{t("publishEnvironmentHint")}</span>
        <div className={css.envGrid} role="radiogroup" aria-label={t("publishEnvironment")}>
          {DEPLOY_ENVIRONMENT_IDS.map((id) => (
            <button
              key={id}
              type="button"
              role="radio"
              aria-checked={environment === id}
              data-selected={environment === id}
              data-environment={id}
              className={css.envCard}
              onClick={() => { onEnvironmentChange(id) }}
            >
              <span className={css.envDot} data-environment={id} />
              <span className={css.envName}>{t(ENVIRONMENT_LABEL_KEYS[id])}</span>
              <span className={css.envId}>{id}</span>
            </button>
          ))}
        </div>
      </div>

      {showDeploymentMode && (
        <div className={css.field}>
          <span className={css.fieldLabel}>{t("deploymentMode")}</span>
          <div className={css.subSelector} role="radiogroup" aria-label={t("deploymentMode")}>
            {DEPLOY_DEPLOYMENT_MODES.map((mode) => (
              <button
                key={mode}
                type="button"
                role="radio"
                aria-checked={deploymentMode === mode}
                data-selected={deploymentMode === mode}
                className={css.subOption}
                onClick={() => { onDeploymentModeChange(mode) }}
              >
                {t(mode === "standalone" ? "modeStandalone" : "modeCloud")}
              </button>
            ))}
          </div>
          {environment !== undefined && (
            <span className={css.profilePreview}>
              {t("profileIdPreview", { id: deployProfileId(deploymentMode, environment) })}
            </span>
          )}
        </div>
      )}
    </>
  );
}
