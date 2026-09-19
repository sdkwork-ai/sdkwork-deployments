/**
 * CreateDeployAppDialog — 发布（发布 deploy_app 应用）对话框（v5）。
 *
 * v5 职责收敛：**本对话框只负责发布，不再创建应用**。应用必须先在
 * `CreateAppDialog` 里创建（名称/类型/图标/封面/预览图），因此这里的
 * 应用步骤退化为「本次发布的目标应用」——由宿主通过 `publishAppId` /
 * `publishAppName`（或关联模式）指定，默认即为入口行上的那个应用。
 *
 * 交互流程：
 *   1. 应用类型 grid（icon + 应用类型名称）
 *   2. 环境与目录：ENVIRONMENT_SPEC 规范环境（开发/测试/预发/演示/线上）+
 *      standalone|cloud 部署形态（决定 dist/<mode>/<envAlias> 产物子树）+
 *      应用根路径（宿主 inspectDirectory 按 sdkwork 规范自动发现表面根路径
 *      并自动完善，无宿主时按 APPLICATION_SPEC 从路径推导）+ 构建产物相对
 *      路径（浏览器类表面随环境/形态联动）+ 框架架构（目录标记自动检测，
 *      带徽标，路径下方手动可改）；「下一步」校验源目录与产物目录存在性
 *   3. 应用：确认发布到哪个应用（默认入口带入的已建应用；也可改关联其他应用）
 *   4. 应用资料（可选）：图标/封面/截图
 *   5. 发布：版本/描述/release notes
 *
 * 持久化严格走 sdkwork-deployments 现有表结构：
 *   deploy_app（name/slug/app_kind/description/metadata）、
 *   deploy_app_platform_target、deploy_app.metadata(JSONB:
 *   category/media/version/releaseNotes/environment/deploymentMode/
 *   applicationCode/surface/framework/buildOutputPath)。
 *
 * 组件为纯 props 输入（两个生成式 client + locale + 宿主端口），不依赖
 * console context，deployments 控制台与 BirdCoder 插件均可复用（高内聚低耦合）。
 */
import { useCallback, useEffect, useMemo, useRef, useState, type FormEvent } from "react";
import type { AppKind, AppResponse, SdkworkDeployAppClient } from "@sdkwork/deployments-app-sdk";
import type { SdkworkDriveAppClient } from "@sdkwork/drive-app-sdk";
import type { DeploymentsLocale } from "@sdkwork/deployments-pc-commons";
import { publishingTranslator, APP_KIND_LABEL_KEYS, type PublishingMessageKey, type PublishingTranslator } from "../i18n.ts";
import {
  createDeployAppPublishingService,
  classifyAppTypeCards,
  classifyFrameworks,
  detectFrameworkId,
  frameworksOfCard,
  isAppSlugConflictError,
  isValidSemver,
  resolveDeployAppType,
  DEPLOY_APP_TYPE_CARDS,
  type CreateDeployAppInput,
  type DeployAppKind,
  type DeployAppMediaGroup,
  type DeployAppTypeOption,
} from "../service/deploy-app-publishing.ts";
import {
  browserDistOutputPath,
  buildOutputExists,
  deriveSurfaceDirectory,
  detectBuildOutputCandidates,
  detectSdkworkProject,
  findDetectedSurface,
  projectProfile,
  repositoryRootOf,
  resolveSourceDirectory,
  shouldSyncEnvironmentBuildOutput,
  type DeployDeploymentMode,
  type DeployEnvironmentId,
  type DeployProjectDetection,
  type DeployProjectInspection,
} from "../service/project-detection.ts";
import { CategoryCascadeSelect } from "./CategoryCascadeSelect.tsx";
import { DeployAppTypeGrid } from "./DeployAppTypeGrid.tsx";
import { DeployFrameworkSelect } from "./DeployFrameworkSelect.tsx";
import {
  DeployAppMediaFields,
  type DeployAppMediaFiles,
} from "./DeployAppMediaFields.tsx";
import { DeployProjectPathBar } from "./DeployProjectPathBar.tsx";
import { DeployProjectDirectoryFields } from "./DeployProjectDirectoryFields.tsx";
import { DeployEnvironmentSelect } from "./DeployEnvironmentSelect.tsx";
import { BuildProgressDialog, type DeployDialogBuildPort } from "./BuildProgressDialog.tsx";
import css from "./create-deploy-app.module.css";

/** 当前登录用户（宿主 IAM 会话投影），用于"按当前用户选择/新建应用"。 */
export interface DeployDialogCurrentUser {
  readonly id: string
  readonly displayName?: string
  readonly avatarUrl?: string
}

export interface CreateDeployAppDialogProps {
  readonly deployClient: SdkworkDeployAppClient
  readonly driveClient: SdkworkDriveAppClient
  readonly locale: DeploymentsLocale
  /**
   * v5: 本次发布的**目标应用** —— 由宿主从列表行带入。应用必须先经
   * `CreateAppDialog` 创建；本对话框只负责把选定的源码目录发布到它上面。
   * 未提供时退回「关联已有应用」选择列表（兼容旧宿主）。
   */
  readonly publishAppId?: string | undefined
  /** v5: 目标应用名称（展示用，缺省时取列表里的名称）。 */
  readonly publishApp?: AppResponse | undefined
  /** v2: 显式初始目录（deployments 控制台直传）。 */
  readonly initialDirectory?: string | undefined
  /** v2: 当前会话/项目的默认目录（宿主下发，自动检测的第一候选）。 */
  readonly defaultDirectory?: string | undefined
  /** v2: 目录检测端口（宿主提供目录列举；缺省时跳过自动发现）。 */
  readonly inspectDirectory?: ((path: string) => Promise<DeployProjectInspection | undefined>) | undefined
  /** v2: 当前登录用户（宿主 IAM 会话）。 */
  readonly currentUser?: DeployDialogCurrentUser | undefined
  /** 目录更换端口（宿主提供：原生选择器 / 浏览器目录选择）。 */
  readonly pickDirectory?: ((current: string | undefined) => Promise<string | undefined>) | undefined
  /** v3.5: 一键打包端口（宿主 sdkwork-app-build 插件；缺省时隐藏打包入口）。 */
  readonly buildPort?: DeployDialogBuildPort | undefined
  /** 主题（驱动组件内建 CSS 变量切换；缺省浅色）。 */
  readonly theme?: ("light" | "dark") | undefined
  readonly onClose: () => void
  readonly onPublished?: ((result: { app: AppResponse; media: DeployAppMediaGroup }) => void) | undefined
}

export interface DeployAppPublishResult {
  app: AppResponse
  media: DeployAppMediaGroup
}

const STEP_COUNT = 5
/** Debounce for directory auto-detection keystrokes (ms). */
const INSPECT_DEBOUNCE_MS = 400

export function CreateDeployAppDialog({
  deployClient,
  driveClient,
  locale,
  publishAppId,
  publishApp,
  initialDirectory,
  defaultDirectory,
  inspectDirectory,
  currentUser,
  pickDirectory,
  buildPort,
  theme = "light",
  onClose,
  onPublished,
}: CreateDeployAppDialogProps) {
  const t = useMemo(() => publishingTranslator(locale), [locale])
  const service = useMemo(
    () => createDeployAppPublishingService({ deployClient, driveClient }),
    [deployClient, driveClient],
  )

  const [step, setStep] = useState(1)
  const [directory, setDirectory] = useState<string | undefined>(initialDirectory ?? defaultDirectory)
  const [cardId, setCardId] = useState<string>()
  const [frameworkId, setFrameworkId] = useState<string>()
  const [buildOutputPath, setBuildOutputPath] = useState("")
  const [detection, setDetection] = useState<DeployProjectDetection>()
  const [detectionRoot, setDetectionRoot] = useState<string>()
  const [inspecting, setInspecting] = useState(false)
  // v5: 应用已在上游创建好；这里只确定「发布到哪个应用」。宿主带入目标应用时
  // 直接锁定；未带入时保留搜索列表让用户挑一个（不再新建）。
  const [apps, setApps] = useState<AppResponse[]>([])
  const [appsSearch, setAppsSearch] = useState("")
  const [appsLoading, setAppsLoading] = useState(false)
  const [associateId, setAssociateId] = useState<string | undefined>(publishAppId)
  const [category, setCategory] = useState<CreateDeployAppInput["category"]>()
  const [media, setMedia] = useState<DeployAppMediaFiles>({ screenshots: {} })
  const [version, setVersion] = useState("1.0.0")
  const [description, setDescription] = useState("")
  const [releaseNotes, setReleaseNotes] = useState("")
  const [environment, setEnvironment] = useState<DeployEnvironmentId>("development")
  const [deploymentMode, setDeploymentMode] = useState<DeployDeploymentMode>("standalone")
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string>()
  const [uploadingLabel, setUploadingLabel] = useState<string>()
  const pickedAppsRef = useRef(false)
  const inspectSequenceRef = useRef(0)
  // v3.5: 一键打包弹窗（打开即启动构建）与最近一次构建结果（成功关闭后复检）。
  const [buildDialog, setBuildDialog] = useState<{ cwd: string; script?: string }>()
  const [buildOutcome, setBuildOutcome] = useState<"succeeded" | "failed" | "cancelled">()

  const type: DeployAppTypeOption | undefined = useMemo(
    () => resolveDeployAppType(cardId, frameworkId),
    [cardId, frameworkId],
  )
  const frameworks = useMemo(() => frameworksOfCard(cardId), [cardId])
  // v4 需求 2：项目画像 + 每张类型卡片的可选性。项目符合 sdkwork 规范且
  // apps/ 表面清单可读时，只放开项目实际提供表面所对应的应用类型；否则全部
  // 放开，由用户自行选择（判定逻辑见 classifyAppTypeCards）。
  const profile = useMemo(() => projectProfile(detection, directory), [detection, directory])
  const cardAvailability = useMemo(
    () => classifyAppTypeCards(DEPLOY_APP_TYPE_CARDS, profile),
    [profile],
  )
  const gatedTypes = profile.sdkwork && profile.surfaces.length > 0
  const cardSupported = useCallback(
    (id: string | undefined) => id === undefined
      ? true
      : cardAvailability.find((entry) => entry.cardId === id)?.supported !== false,
    [cardAvailability],
  )
  const suggestedCardId = useMemo(() => {
    if (detection === undefined) return undefined
    const detected = new Set(profile.surfaces)
    const isDetected = (id: string | undefined) => {
      const surface = DEPLOY_APP_TYPE_CARDS.find((candidate) => candidate.id === id)?.surface
      return surface !== undefined && detected.has(surface)
    }
    // 已选卡片且其表面已被检测到 → 保留「检测到」徽标；尚未选择时推荐第一个
    // 被检测到的表面，让用户一眼看出项目实际提供哪些应用。
    if (cardId !== undefined) return isDetected(cardId) ? cardId : undefined
    return DEPLOY_APP_TYPE_CARDS.find((card) => isDetected(card.id))?.id
  }, [detection, profile, cardId])
  const matchedSurfacePath = useMemo(() => {
    if (detection === undefined || type?.surface === undefined || directory === undefined) return undefined
    return resolveSourceDirectory(detection, type.surface, detectionRoot ?? directory)
  }, [detection, type, directory, detectionRoot])
  // v3.3: 规范路径推导（不依赖宿主列举）—— 目录是 sdkwork-<code> 仓库根时，
  // 按 APPLICATION_SPEC 推导 apps/sdkwork-<code>-<suffix> 表面根；当前目录已
  // 是其他表面根（apps/ 下）时推导同级表面目录。检测结果可用时以检测为准。
  const specSurfacePath = useMemo(
    () => (directory === undefined || type?.surface === undefined
      ? undefined
      : deriveSurfaceDirectory(directory, type.surface)),
    [directory, type],
  )
  // v3: 构建产物路径验证与候选 —— 依据选中表面目录的子目录列举。v4：
  // 无表面的类型（API 服务 / 静态资源）发布仓库根目录，改用根目录列举，
  // 使其框架与产物检测同样可用。
  const matchedSurfaceChildren = useMemo(
    () => type?.surface === undefined
      ? detection?.rootChildDirectories
      : findDetectedSurface(detection, type.surface)?.childDirectories,
    [detection, type],
  )
  const buildOutputDetected = useMemo(
    () => buildOutputExists(buildOutputPath, matchedSurfaceChildren),
    [buildOutputPath, matchedSurfaceChildren],
  )
  const buildCandidates = useMemo(
    () => detectBuildOutputCandidates(matchedSurfaceChildren),
    [matchedSurfaceChildren],
  )
  // v3.2: 依据目录标记信号自动检测框架（.dart_tool→Flutter、unpackage→uni-app、
  // src-tauri→Tauri、android/ios→RN 等，见 detectDirectories 注册表）。
  const autoDetectedId = useMemo(
    () => detectFrameworkId(frameworks, matchedSurfaceChildren),
    [frameworks, matchedSurfaceChildren],
  )
  // v4 需求 4：所选项目必须符合项目自身的应用架构 —— 项目表面目录已给出
  // 决定性架构信号时，标识目录缺席的框架判为架构冲突并禁止选择。
  const frameworkConflicts = useMemo(
    () => classifyFrameworks(frameworks, matchedSurfaceChildren)
      .filter((entry) => entry.conflicting)
      .map((entry) => entry.frameworkId),
    [frameworks, matchedSurfaceChildren],
  )
  // v3.4: 浏览器类表面（pc/h5/static）的构建产物目录随 `<mode>.<environment>`
  // 组合变化：dist/<deploymentProfile>/<envAlias>（FRONTEND_CODE_SPEC §7）——
  // standalone 与 cloud 各自独立子树，绝不裸 dist/。其他表面不受环境影响。
  const envBuildOutput = useMemo(
    () => (type?.surface === "pc" || type?.surface === "h5" || type?.surface === "static"
      ? browserDistOutputPath(deploymentMode, environment)
      : undefined),
    [type, deploymentMode, environment],
  )
  const frameworkDefaultBuildOutput = useMemo(
    () => frameworks.find((candidate) => candidate.id === frameworkId)?.buildOutputPath,
    [frameworks, frameworkId],
  )

  const selectCard = (nextCardId: string) => {
    setCardId(nextCardId)
    const card = DEPLOY_APP_TYPE_CARDS.find((candidate) => candidate.id === nextCardId)
    const nextFrameworkId = card?.defaultFrameworkId
    setFrameworkId(nextFrameworkId)
    const framework = card?.frameworks.find((candidate) => candidate.id === nextFrameworkId)
    setBuildOutputPath(framework?.buildOutputPath ?? "")
  }

  const selectFramework = (nextFrameworkId: string) => {
    setFrameworkId(nextFrameworkId)
    const framework = frameworks.find((candidate) => candidate.id === nextFrameworkId)
    setBuildOutputPath(framework?.buildOutputPath ?? "")
  }

  // v5: 目标应用 —— 宿主带入的应用优先；否则取关联列表中被选中的那条。
  const targetApp = useMemo<AppResponse | undefined>(
    () => publishApp ?? apps.find((candidate) => candidate.id === associateId),
    [publishApp, apps, associateId],
  )
  const targetAppId = publishApp?.id ?? associateId
  const targetAppName = targetApp?.name ?? associateId

  const loadApps = async (keyword: string) => {
    setAppsLoading(true)
    setError(undefined)
    try {
      const result = await service.listApps({ page: 1, pageSize: 50, keyword: keyword.trim() || undefined })
      setApps(result.items)
    } catch (cause) {
      setError(errorText(cause, t))
    } finally {
      setAppsLoading(false)
    }
  }

  const searchApps = (event: FormEvent) => {
    event.preventDefault()
    void loadApps(appsSearch)
  }

  const changeDirectory = async () => {
    if (!pickDirectory) return
    try {
      const next = await pickDirectory(directory)
      if (next) setDirectory(next)
    } catch (cause) {
      setError(errorText(cause, t))
    }
  }

  // v2: 目录自动检测 —— 目录变化（含初始默认目录）后防抖触发宿主 inspection。
  // v4: 目录位于 apps/<surface>/ 内部时补一次所属仓库根列举 —— 否则同级表面
  // 不可见，「本项目支持哪些应用类型」会退化成「无法证明」而全部放开。
  useEffect(() => {
    const path = directory?.trim()
    if (inspectDirectory === undefined || path === undefined || path === "") {
      setDetection(undefined)
      setDetectionRoot(undefined)
      return
    }
    const sequence = ++inspectSequenceRef.current
    const live = () => inspectSequenceRef.current === sequence
    setInspecting(true)
    const timer = window.setTimeout(() => {
      void (async () => {
        try {
          const inspection = await inspectDirectory(path)
          if (!live()) return
          const hasSurfaces = (inspection?.appsChildDirectories?.length ?? 0) > 0
          const repositoryRoot = hasSurfaces ? undefined : repositoryRootOf(path)
          const rootInspection = repositoryRoot === undefined
            ? undefined
            : await inspectDirectory(repositoryRoot)
          if (!live()) return
          // 仓库根列举成功时以它为准：它同时给出同级表面清单与各表面的子目录。
          const resolved = rootInspection ?? inspection
          if (resolved === undefined) {
            setDetection(undefined)
            setDetectionRoot(undefined)
            return
          }
          setDetection(detectSdkworkProject(resolved))
          setDetectionRoot(resolved.rootPath)
        } catch {
          if (!live()) return
          setDetection(undefined)
          setDetectionRoot(undefined)
        } finally {
          if (live()) setInspecting(false)
        }
      })()
    }, INSPECT_DEBOUNCE_MS)
    return () => { window.clearTimeout(timer) }
  }, [directory, inspectDirectory])

  // v3.3: 目录自动完善 —— 两条互补路径：
  // ① 检测驱动（宿主列举验证过的表面根）：仅在目录为空/仍是检测根/仍是上次
  //    自动填充值时应用，不覆盖手动修改；
  // ② 规范推导驱动（specSurfacePath，纯路径推导）：目录本身是规范仓库根或
  //    apps/ 下的表面目录时始终完善 —— 这是确定性推导，选择应用类型后立即
  //    得到正确的表面根路径（如 h5 → <repo>/apps/sdkwork-<code>-h5）。
  const lastAutoDirectoryRef = useRef<string | undefined>(undefined)
  useEffect(() => {
    const target = matchedSurfacePath ?? specSurfacePath
    if (target === undefined || target === directory) return
    if (specSurfacePath === undefined) {
      const autoFillable =
        directory === undefined
        || directory.trim() === ""
        || directory === detectionRoot
        || directory === lastAutoDirectoryRef.current
      if (!autoFillable) return
    }
    lastAutoDirectoryRef.current = target
    setDirectory(target)
  }, [matchedSurfacePath, specSurfacePath, detectionRoot, directory])

  // v3.4: 产物目录跟随环境/部署形态切换（仅浏览器类表面）—— 当前值仍是框架
  // 默认值或上次自动值（未被手动修改）时应用新的 dist/<mode>/<envAlias>，
  // 手动自定义的路径不被覆盖（判定逻辑见 shouldSyncEnvironmentBuildOutput）。
  const lastEnvBuildOutputRef = useRef<string | undefined>(undefined)
  useEffect(() => {
    if (envBuildOutput === undefined) return
    // buildOutputPath 刻意读取最新值：仅环境/形态/框架变化时联动。
    // eslint-disable-next-line react-hooks/exhaustive-deps
    if (!shouldSyncEnvironmentBuildOutput(buildOutputPath, frameworkDefaultBuildOutput, lastEnvBuildOutputRef.current)) return
    lastEnvBuildOutputRef.current = envBuildOutput
    setBuildOutputPath(envBuildOutput)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [envBuildOutput, frameworkDefaultBuildOutput])

  // v3.4: 候选 chip 拦截 —— 浏览器类表面点击裸 "dist" 候选时，specs 认定裸
  // dist/ 不是有效产物目录（APP_CLIENT_ARCHITECTURE_ALIGNMENT_SPEC §2.2），
  // 改落当前环境/形态对应的 dist/<mode>/<envAlias>；其余值原样透传。
  const handleBuildOutputChange = (value: string) => {
    if (envBuildOutput !== undefined && value.trim() === "dist") {
      lastEnvBuildOutputRef.current = envBuildOutput
      setBuildOutputPath(envBuildOutput)
      return
    }
    setBuildOutputPath(value)
  }

  // v3.2: 框架自动检测 —— 目录标记命中注册表时自动应用并同步构建产物默认
  // 值；用户手动选择不被覆盖（仅在检测信号或应用类型变化时重新应用）。
  useEffect(() => {
    if (autoDetectedId === undefined) return
    setFrameworkId(autoDetectedId)
    setBuildOutputPath(frameworks.find((candidate) => candidate.id === autoDetectedId)?.buildOutputPath ?? "")
    // cardId 入依赖：切卡后重放目录信号（优先于 selectCard 的默认框架）。
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [autoDetectedId, cardId])

  // v4：目录换成另一个项目后，原先选中的应用类型可能已不在新项目的 apps/
  // 表面之内 —— 这会直接违反「必须符合项目规范」，因此清空类型与框架选择，
  // 让用户重新选一个受支持的。（检测中 detection 为 undefined，不会误清。）
  useEffect(() => {
    if (cardId === undefined || cardSupported(cardId)) return
    setCardId(undefined)
    setFrameworkId(undefined)
    setBuildOutputPath("")
  }, [cardId, cardSupported])

  const canNext = (): boolean => {
    if (step === 1) return cardId !== undefined && cardSupported(cardId)
    if (step === 2) return Boolean(directory?.trim()) && frameworkId !== undefined
    // v5: 第 3 步只确认发布目标应用 —— 应用必须已存在（由上游创建或列表中选中）。
    if (step === 3) return Boolean(targetAppId)
    if (step === 4) return true
    return isValidSemver(version)
  }

  // v3.4: 目录步骤「下一步」前的存在性检查（宿主桥可用时）—— 源目录必须
  // 能被宿主列举成功（detection 非 undefined），构建产物目录按表面子目录
  // 列举判定存在性；无法判定（无列举能力）时放行，由发布时兜底。
  const stepTwoDirectoryProblem = (): string | undefined => {
    if (inspectDirectory !== undefined) {
      if (inspecting) return t("publishCheckingDirectories")
      if (detection === undefined) return t("sourceDirectoryMissing")
    }
    if (buildOutputDetected === false) {
      return t("buildOutputMissing", { path: buildOutputPath.trim() })
    }
    return undefined
  }

  const next = () => {
    if (!canNext()) {
      setError(t("publishRequiredFields"))
      return
    }
    if (step === 2) {
      const directoryProblem = stepTwoDirectoryProblem()
      if (directoryProblem !== undefined) {
        setError(directoryProblem)
        return
      }
    }
    setError(undefined)
    setStep((current) => Math.min(STEP_COUNT, current + 1))
  }

  const previous = () => {
    setError(undefined)
    setStep((current) => Math.max(1, current - 1))
  }

  // v3.5: 一键打包弹窗关闭 —— 构建成功时复检目录，让 dist 产物检测即时翻绿，
  // 用户无需手动改动路径再触发防抖检测。
  const closeBuildDialog = () => {
    const succeeded = buildOutcome === "succeeded"
    setBuildDialog(undefined)
    setBuildOutcome(undefined)
    if (succeeded) {
      const current = directory
      setDirectory(undefined)
      window.setTimeout(() => { setDirectory(current) }, 0)
    }
  }

  const uploadMedia = async (appId: string, files: DeployAppMediaFiles): Promise<DeployAppMediaGroup> => {
    const upload = async (kind: "icon" | "cover" | "screenshot", file: File, targetKey?: string) => {
      setUploadingLabel(t("mediaUploading", { name: file.name }))
      const ref = await service.uploadMedia({
        kind,
        file,
        fileName: file.name,
        contentType: file.type || "application/octet-stream",
        targetKey,
      }, appId)
      return ref
    }

    const group: { icon?: DeployAppMediaGroup["icon"]; cover?: DeployAppMediaGroup["cover"]; screenshots: DeployAppMediaGroup["screenshots"] } = { screenshots: {} }
    if (files.icon) {
      group.icon = await upload("icon", files.icon)
    }
    if (files.cover) {
      group.cover = await upload("cover", files.cover)
    }
    for (const [targetKey, fileList] of Object.entries(files.screenshots)) {
      group.screenshots[targetKey] = []
      for (const file of fileList) {
        const ref = await upload("screenshot", file, targetKey)
        group.screenshots[targetKey] = [...group.screenshots[targetKey], ref]
      }
    }
    return group
  }

  const submit = async () => {
    if (directory === undefined || type === undefined || !isValidSemver(version)) {
      setError(t("publishRequiredFields"))
      return
    }
    // v5: 发布只针对已存在的应用 —— 没有目标应用就没有可发布的对象。
    // 缺目标应用时不动「创建」，而是把用户指回创建入口（职责分离）。
    if (targetAppId === undefined) {
      setError(t("publishDisabledNoApp"))
      return
    }
    // 发布前复检目录存在性（第 2 步校验的兜底，防止后续步骤中目录被删）。
    const directoryProblem = stepTwoDirectoryProblem()
    if (directoryProblem !== undefined) {
      setError(directoryProblem)
      return
    }
    setBusy(true)
    setError(undefined)
    setUploadingLabel(undefined)
    try {
      const base: CreateDeployAppInput = {
        sourceDirectory: directory,
        // 应用已存在：一律走关联更新，绝不在此新建应用。
        associateAppId: targetAppId,
        type,
        framework: frameworkId,
        buildOutputPath: buildOutputPath.trim(),
        category,
        version,
        description,
        releaseNotes,
        environment,
        deploymentMode,
        applicationCode: detection?.applicationCode,
      }

      // 1) 更新目标应用的发布元数据（版本/环境/产物/框架/分类）。
      const app = await service.createApp(base)

      // 2) 上传媒体（图标/封面/截图 → Drive），app 存在后以 appId 为资源锚点。
      let mediaGroup: DeployAppMediaGroup = { screenshots: {} }
      if (media.icon || media.cover || Object.keys(media.screenshots).length > 0) {
        mediaGroup = await uploadMedia(app.id, media)
        // 3) 回写 metadata.media（保留原有 metadata，避免清掉创建阶段写入的字段）。
        await deployClient.app.update(app.id, {
          metadata: { ...(app.metadata ?? {}), ...service.buildMetadata(base), media: mediaGroup },
        })
      }

      onPublished?.({ app, media: mediaGroup })
      setBusy(false)
    } catch (cause) {
      setError(errorText(cause, t))
      setBusy(false)
    }
  }

  return (
    <div className={css.publishDialog} data-theme={theme} role="presentation" onMouseDown={(event) => { if (event.target === event.currentTarget && !busy) onClose() }}>
      <div className={css.dialog} role="dialog" aria-modal="true" aria-label={t("publishApp")}>
        <header className={css.header}>
          <div className={css.headerText}>
            <h2>{t("publishApp")}</h2>
            <p>{t("publishAppDescription")}</p>
          </div>
          <button type="button" className={css.closeButton} title={t("close")} aria-label={t("close")} onClick={onClose}>
            ×
          </button>
        </header>

        <div className={css.steps} aria-label={t("step", { step: String(step), total: String(STEP_COUNT) })}>
          {Array.from({ length: STEP_COUNT }, (_, index) => (
            <span key={index} className={css.stepDot} data-active={index < step} />
          ))}
        </div>

        {/* v4 需求 1：当前选中的项目路径常驻显示（5 个步骤都在），并给出该
            目录的规范判定与应用表面摘要。 */}
        <DeployProjectPathBar
          directory={directory}
          inspecting={inspecting}
          detection={detection}
          profile={profile}
          t={t}
          onChangeDirectory={() => { void changeDirectory() }}
        />

        <div className={css.body}>
          {step === 1 && (
            <DeployAppTypeGrid
              cardId={cardId}
              suggestedCardId={suggestedCardId}
              availability={cardAvailability}
              gated={gatedTypes}
              t={t}
              onChange={selectCard}
            />
          )}

          {step === 2 && (
            <>
              {/* v3.4: 环境与部署形态提前到目录步骤 —— 不同的
                  `<standalone|cloud>.<environment>` 组合对应不同的构建产物
                  dist 子目录（dist/<mode>/<envAlias>，FRONTEND_CODE_SPEC §7），
                  必须先于产物路径字段确定。 */}
              <DeployEnvironmentSelect
                environment={environment}
                deploymentMode={deploymentMode}
                onEnvironmentChange={setEnvironment}
                onDeploymentModeChange={setDeploymentMode}
                t={t}
              />
              <DeployProjectDirectoryFields
                directory={directory ?? ""}
                buildOutputPath={buildOutputPath}
                detection={detection}
                inspecting={inspecting}
                selectedSurface={type?.surface}
                matchedSurfacePath={matchedSurfacePath}
                buildOutputDetected={buildOutputDetected}
                buildCandidates={buildCandidates}
                t={t}
                onDirectoryChange={setDirectory}
                onBuildOutputChange={handleBuildOutputChange}
                onChangeDirectoryClick={() => { void changeDirectory() }}
                onReinspect={() => {
                  // 触发重检测：先清空再写回同一目录，走防抖 effect。
                  const current = directory
                  setDirectory(undefined)
                  window.setTimeout(() => { setDirectory(current) }, 0)
                }}
              />
              {/* v3.2: 框架选择并入目录步骤（路径下方），依据目录信号自动检测。
                  v4：与项目架构不符的框架由 conflictingIds 置灰。 */}
              <DeployFrameworkSelect
                frameworks={frameworks}
                frameworkId={frameworkId}
                autoDetectedId={autoDetectedId}
                conflictingIds={frameworkConflicts}
                t={t}
                onChange={selectFramework}
              />
              {/* v3.5: 产物缺失 + 宿主构建端口可用 → 一键打包入口。构建目标
                  优先规范推导的表面根（源目录是仓库根时自动落到对应表面）。 */}
              {buildPort !== undefined && directory !== undefined && buildOutputDetected === false && (
                <div className={css.buildTriggerRow}>
                  <span className={css.fieldHint}>{t("buildTriggerHint")}</span>
                  <button
                    type="button"
                    className={css.secondaryButton}
                    onClick={() => {
                      setBuildOutcome(undefined)
                      setBuildDialog({ cwd: matchedSurfacePath ?? specSurfacePath ?? directory })
                    }}
                  >
                    {t("buildTrigger")}
                  </button>
                </div>
              )}
            </>
          )}

          {step === 3 && (
            <>
              <div className={css.publishAsRow}>
                <span className={css.fieldLabel}>{t("publishAs")}</span>
                <span className={css.userChip}>
                  {currentUser?.avatarUrl !== undefined && <img className={css.userAvatar} src={currentUser.avatarUrl} alt="" />}
                  <span>{currentUser?.displayName ?? currentUser?.id ?? t("userUnknown")}</span>
                </span>
              </div>
              {/* v5: 应用只读确认 —— 应用已在创建流程登记，这里只显示发布目标；
                  未带入目标应用时（兼容旧宿主）允许从已建应用里挑一个，但仍不新建。 */}
              <StepPublishTarget
                targetApp={targetApp}
                targetAppId={targetAppId}
                targetAppName={targetAppName}
                apps={apps}
                appsLoading={appsLoading}
                appsSearch={appsSearch}
                pickedRef={pickedAppsRef}
                t={t}
                onAppsSearchChange={setAppsSearch}
                onSearchApps={searchApps}
                onLoadApps={(keyword) => { void loadApps(keyword) }}
                onAssociateChange={setAssociateId}
              />
              <div className={css.field}>
                <span className={css.fieldLabel}>{t("category")}</span>
                <span className={css.fieldHint}>{t("categoryHint")}</span>
                <CategoryCascadeSelect appKind={type?.appKind as AppKind | undefined} value={category} onChange={setCategory} t={t} theme={theme} />
              </div>
            </>
          )}

          {step === 4 && (
            <DeployAppMediaFields
              value={media}
              onChange={setMedia}
              t={t}
              appKind={(type?.appKind as DeployAppKind | undefined) ?? publishApp?.appKind}
            />
          )}

          {step === 5 && (
            <>
              {/* v3.4: 环境与部署形态已前移到目录步骤（步骤 2）——
                  产物 dist 路径依赖该组合，此处仅保留版本与发布说明。 */}
              <div className={css.field}>
                <span className={css.fieldLabel}>{t("version")}</span>
                <input
                  className={css.input}
                  value={version}
                  placeholder={t("versionPlaceholder")}
                  onChange={(event) => { setVersion(event.target.value) }}
                  aria-invalid={version.trim() !== "" && !isValidSemver(version)}
                />
                <span className={css.fieldHint}>{t("versionHint")}</span>
                {version.trim() !== "" && !isValidSemver(version) && (
                  <span className={css.errorBanner} role="alert">{t("versionError")}</span>
                )}
              </div>
              <div className={css.field}>
                <span className={css.fieldLabel}>{t("description")}</span>
                <textarea
                  className={css.textarea}
                  value={description}
                  placeholder={t("descriptionPlaceholder")}
                  onChange={(event) => { setDescription(event.target.value) }}
                />
              </div>
              <div className={css.field}>
                <span className={css.fieldLabel}>{t("releaseNotes")}</span>
                <textarea
                  className={css.textarea}
                  value={releaseNotes}
                  placeholder={t("releaseNotesPlaceholder")}
                  onChange={(event) => { setReleaseNotes(event.target.value) }}
                />
              </div>
            </>
          )}
        </div>

        <footer className={css.footer}>
          {error && <div className={css.errorBanner} role="alert">{error}</div>}
          {uploadingLabel && <div className={css.uploadingText}>{uploadingLabel}</div>}
          {/* spacer 仅在没有 error 横幅时把按钮推到右侧；errorBanner 与 spacer
              同为 flex:1 会各分一半自由宽度，提示文案被挤压换行（用户可见回归）。
              uploadingText 无 flex:1，单独出现时仍需 spacer 维持按钮靠右。 */}
          {!error && <div className={css.footerSpacer} />}
          {step > 1 && (
            <button type="button" className={css.secondaryButton} disabled={busy} onClick={previous}>
              {t("previous")}
            </button>
          )}
          {step < STEP_COUNT
            ? (
              <button type="button" className={css.primaryButton} disabled={busy} onClick={next}>
                {t("next")}
              </button>
            )
            : (
              <button type="button" className={css.primaryButton} disabled={busy || !canNext()} onClick={() => { void submit() }}>
                {busy ? t("publishing") : t("publish")}
              </button>
            )}
        </footer>
      </div>

      {/* v3.5: 一键打包进度弹窗（覆盖在发布对话框之上，自带遮罩）。 */}
      {buildDialog !== undefined && buildPort !== undefined && (
        <BuildProgressDialog
          locale={locale}
          theme={theme}
          cwd={buildDialog.cwd}
          script={buildDialog.script}
          port={buildPort}
          onFinished={setBuildOutcome}
          onClose={closeBuildDialog}
        />
      )}
    </div>
  )
}

interface StepPublishTargetProps {
  targetApp: AppResponse | undefined
  targetAppId: string | undefined
  targetAppName: string | undefined
  apps: readonly AppResponse[]
  appsLoading: boolean
  appsSearch: string
  pickedRef: { current: boolean }
  t: PublishingTranslator
  onAppsSearchChange: (value: string) => void
  onSearchApps: (event: FormEvent) => void
  onLoadApps: (keyword: string) => void
  onAssociateChange: (value: string) => void
}

/**
 * v5 第 3 步：确认本次发布的目标应用。
 *
 * 应用已在 `CreateAppDialog` 里登记好（名称 / 图标 / 封面 / 预览图），因此这里
 * 是**只读确认**：显示应用名称、类型与 slug。只有在宿主没有带入目标应用时
 * （兼容旧宿主调用），才退回已建应用的搜索列表让用户挑一个 —— 任何情况下
 * 都不在这里新建应用，从而保证「先创建，再发布」的顺序不可绕过。
 */
function StepPublishTarget(props: StepPublishTargetProps) {
  const {
    targetApp, targetAppId, targetAppName, apps, appsLoading, appsSearch, pickedRef,
    t, onAppsSearchChange, onSearchApps, onLoadApps, onAssociateChange,
  } = props
  const locked = targetApp !== undefined

  return (
    <div className={css.field}>
      <span className={css.fieldLabel}>{t("publishTargetApp")}</span>
      {locked ? (
        <div className={css.appList}>
          <div className={css.appRow} data-selected="true">
            <span className={css.appRowMeta}>
              <strong>{targetApp.name}</strong>
              <small>{appKindLabel(targetApp.appKind, t)}{targetApp.slug ? ` · ${targetApp.slug}` : ""}</small>
            </span>
          </div>
        </div>
      ) : (
        <>
          <span className={css.fieldHint}>{t("publishTargetPickHint")}</span>
          {targetAppId !== undefined && targetAppName === undefined && (
            <div className={css.appEmpty}>{targetAppId}</div>
          )}
          <form className={css.appSearch} onSubmit={onSearchApps}>
            <input
              className={css.input}
              value={appsSearch}
              placeholder={t("searchApps")}
              onChange={(event) => { onAppsSearchChange(event.target.value) }}
            />
            <button type="submit" className={css.secondaryButton}>{t("searchApps")}</button>
          </form>
          <div className={css.appList} aria-busy={appsLoading}>
            {appsLoading && <div className={css.appEmpty}>{t("appSearching")}</div>}
            {!appsLoading && apps.length === 0 && <div className={css.appEmpty}>{t("noApps")}</div>}
            {!appsLoading && apps.map((app) => (
              <button
                key={app.id}
                type="button"
                className={css.appRow}
                data-selected={targetAppId === app.id}
                onClick={() => { onAssociateChange(app.id) }}
                onFocus={() => {
                  // 首次展开即拉取一次列表，避免用户还得先点一次「搜索」。
                  if (!pickedRef.current) {
                    pickedRef.current = true
                    onLoadApps("")
                  }
                }}
              >
                <span className={css.appRowMeta}>
                  <strong>{app.name}</strong>
                  <small>{appKindLabel(app.appKind, t)}{app.slug ? ` · ${app.slug}` : ""}</small>
                </span>
              </button>
            ))}
          </div>
        </>
      )}
    </div>
  )
}

function errorText(cause: unknown, t: PublishingTranslator): string {
  // Same routine conflict as in CreateAppDialog: an app whose slug is already
  // taken must tell the operator what to change, not echo the server's rule text.
  if (isAppSlugConflictError(cause)) {
    return t("appSlugConflict")
  }
  const message = cause instanceof Error && cause.message ? cause.message : String(cause)
  return t("publishFailed", { message })
}

/**
 * deploy_app.app_kind 枚举的本地化展示；服务端出现映射表未覆盖的新枚举值时
 * 回退原文，保证列表不因枚举演进而空白。
 */
function appKindLabel(appKind: AppKind, t: PublishingTranslator): string {
  const key: PublishingMessageKey | undefined = APP_KIND_LABEL_KEYS[appKind];
  return key !== undefined ? t(key) : appKind;
}
