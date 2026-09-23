/**
 * ⚠️ 未落地（v7）：本文件没有任何生产消费者，只被已 park 的
 * `tests/pending/publish-field-validation-v7.spec.ts.pending` 引用（见该目录 README）。
 * 它缺的那几个 `field*` / `publishMissingFields` 文案也还没进 i18n，所以它自身
 * 仍带着 typecheck 报错——那是"未落地"的标记，不是需要绕过的噪声。
 *
 * 发布对话框的**字段级**校验（v7）。
 *
 * 为什么单独成模块：v7 之前这里是一个 `canNext(): boolean`，校验结果被压成
 * 一个布尔，界面只能回一句「请先填写必填项再发布」—— 用户既不知道哪些字段
 * 必填，也不知道是哪一项没填（实测反馈）。改成「字段 → 原因」之后，判据必须
 * 能被**单独执行**：写在校验函数里，而不是散在组件渲染分支里（组件里的一行
 * `if` 是单测盲区，只有渲染级断言才看得见）。
 *
 * 本模块**不产出本地化文本**：每个问题只带 `messageKey` + 参数，文案由视图层
 * 用 `t()` 渲染。这样判据与语言无关，测试也不必断言某句中文。
 */
import type { PublishingMessageKey, PublishingTranslator } from "../i18n.ts";
import { isValidSemver } from "./deploy-app-publishing.ts";

/**
 * 需要用户补齐、且能落到**具体控件**上的字段。
 *
 * 「发布表面」（多候选 app_kind）不在其中：它没有可聚焦的输入框语义，解析不
 * 出来时走 `appTypeLockedUnknown` / `appTypeLockedAmbiguous` 专用文案。
 */
export type PublishFieldId = "directory" | "buildOutput" | "framework" | "targetApp" | "version"

/** 一个字段的问题：文案键 + 插值参数。 */
export interface PublishFieldProblem {
  readonly messageKey: PublishingMessageKey
  readonly params?: Readonly<Record<string, string>> | undefined
}

/** 校验输入快照（组件 state 的投影；纯数据，便于在测试里直接构造）。 */
export interface PublishFieldSnapshot {
  /** 源目录（应用根路径）。 */
  readonly directory: string | undefined
  /** 构建产物相对目录。 */
  readonly buildOutputPath: string
  /**
   * 产物目录在表面子目录里的存在性：
   * true = 已检测到，false = 列举过但不存在，undefined = 无从判定（放行）。
   */
  readonly buildOutputDetected: boolean | undefined
  readonly frameworkId: string | undefined
  readonly targetAppId: string | undefined
  readonly version: string
  /** 宿主是否提供目录列举能力（`inspectDirectory`）。 */
  readonly inspectionAvailable: boolean
  /** 是否正在列举中。 */
  readonly inspecting: boolean
  /** 当前目录是否已被成功列举（`detection !== undefined`）。 */
  readonly inspected: boolean
}

/** 字段级问题集合；空对象表示该步可放行。 */
export type PublishFieldProblems = Partial<Record<PublishFieldId, PublishFieldProblem>>

/** 字段级错误汇总时用的字段名（与标签一致，用户能一眼对上界面）。 */
export const PUBLISH_FIELD_LABEL_KEYS: Readonly<Record<PublishFieldId, PublishingMessageKey>> = {
  directory: "sourceDirectory",
  buildOutput: "buildOutputPath",
  framework: "frameworkLabel",
  targetApp: "publishTargetApp",
  version: "version",
}

/** 每个字段所属的步骤 —— 跨步骤报错时先把用户送回那一步，再滚动聚焦。 */
export const PUBLISH_FIELD_STEP: Readonly<Record<PublishFieldId, number>> = {
  directory: 1,
  buildOutput: 1,
  framework: 1,
  targetApp: 2,
  version: 4,
}

/**
 * 校验第 `step` 步的字段，返回「字段 → 它自己的问题」。
 *
 * 存在性检查（源目录能否被列举、产物目录是否真的存在）也归到**对应字段**上：
 * v7 之前它们是步骤级字符串，用户同样不知道该去改哪个框。
 * 没有列举能力（`inspectionAvailable === false`）时不判定存在性，由发布时兜底。
 */
export function validatePublishFields(step: number, snapshot: PublishFieldSnapshot): PublishFieldProblems {
  const problems: { -readonly [K in PublishFieldId]?: PublishFieldProblem } = {}

  if (step === 1) {
    if (!snapshot.directory?.trim()) {
      problems.directory = { messageKey: "fieldErrorSourceDirectory" }
    } else if (snapshot.inspectionAvailable) {
      if (snapshot.inspecting) problems.directory = { messageKey: "publishCheckingDirectories" }
      else if (!snapshot.inspected) problems.directory = { messageKey: "sourceDirectoryMissing" }
    }

    const buildOutput = snapshot.buildOutputPath.trim()
    if (buildOutput === "") {
      problems.buildOutput = { messageKey: "fieldErrorBuildOutput" }
    } else if (snapshot.buildOutputDetected === false) {
      problems.buildOutput = { messageKey: "buildOutputMissing", params: { path: buildOutput } }
    }

    if (snapshot.frameworkId === undefined) {
      problems.framework = { messageKey: "fieldErrorFramework" }
    }
  }

  if (step === 2 && snapshot.targetAppId === undefined) {
    problems.targetApp = { messageKey: "fieldErrorTargetApp" }
  }

  if (step === 4 && !isValidSemver(snapshot.version)) {
    problems.version = { messageKey: "versionError" }
  }

  return problems
}

/** 把问题渲染成文案（视图层唯一入口，保证本地化只发生在一处）。 */
export function publishProblemText(problem: PublishFieldProblem, t: PublishingTranslator): string {
  return problem.params === undefined ? t(problem.messageKey) : t(problem.messageKey, problem.params)
}

/**
 * 页脚汇总：把字段级问题指名道姓地说出来（「还有 2 项必填未完成：源码目录、
 * 框架 / 架构」），而不是一句「请填写必填项」。
 */
export function publishProblemsSummary(
  problems: PublishFieldProblems,
  separator: string,
  t: PublishingTranslator,
): string | undefined {
  const ids = Object.keys(problems) as PublishFieldId[]
  if (ids.length === 0) return undefined
  const labels = ids.map((id) => t(PUBLISH_FIELD_LABEL_KEYS[id]))
  return t("publishMissingFields", { count: String(labels.length), fields: labels.join(separator) })
}
