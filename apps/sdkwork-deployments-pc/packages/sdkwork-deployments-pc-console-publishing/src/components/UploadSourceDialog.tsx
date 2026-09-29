/**
 * UploadSourceDialog — 应用行上的「接入源码」命令。
 *
 * ## 为什么第一个问题不再是「代码从哪来」
 *
 * 一个 `deploy_app` 现在可以同时带 PC、H5、小程序等多份源码，每份源码各自服务
 * 一类客户端，并且各自有自己的排位（`deploy_app_source_spec` + 每端路由子表）。
 * 于是「上传代码」这个动作必须先回答**这是哪一份源码**，否则传上去的字节属于
 * 哪个规格、会不会对外服务都无从判断。
 *
 * 同时契约把「源码」定义成**内容源**而不是制品：
 * `BindAppSourceSpecSourceRequest.source` 是一个二选一联合 ——
 * `DriveDirectorySource`（空间根 / 文件夹 + 内容模式）或 `KnowledgebaseWikiSource`
 * （已发布的 Wiki 版本）。所以本对话框有两个顶层动作，顺序就是它们的优先级：
 *
 *   1. **绑定源码来源**（默认，对齐契约）—— ① 选目标规格 → ② 选来源类型
 *      （网盘目录 / 知识库 Wiki）→ 落到 `bindSourceSpecSource`，**直接产生
 *      `spec.source`**。这是唯一会让一份源码真正开始服务的动作：没有来源的规格
 *      不产出任何路由规则，请求会继续往后回退。
 *   2. **上传制品 / 发版**（保留）—— 本地压缩包 / Git 仓库 / 网盘里已有的压缩包，
 *      走应用级的 artifact → release → deployment 链路。它是**应用级**动作、
 *      **不绑定任何规格**，面板里明写这一点 —— 否则「传上去了却还是 404」
 *      没有解释。
 *
 * ## 两条容易踩的坑
 *
 * - **目录 vs 文件是两种语义，不是两种大小。** 绑定来源要的是**目录**
 *   （`DriveDirectorySource.root` 只接受空间根或文件夹），上传制品要的是**文件**
 *   （目录没有单一字节流）。同一个 `DriveNodePickerDialog` 因此在两个分支里的
 *   取舍**正好相反**：一个拒绝文件，一个拒绝目录。把两者混成一个分支，就会出现
 *   「选完目录点确认，服务端收到一个文件夹 uuid」这种越界请求。
 * - **`File` 不是 `Blob` 的窄化替身**：`DriveUploaderBlobLike` 允许宿主换成
 *   文件系统句柄，所以校验一律读 `file.size` / `file.type`，不假设 `File` 特有的
 *   `lastModified`。
 *
 * ## 挂载点
 *
 * 本组件为纯 props 输入（生成式 client + locale），被两处挂载：应用行的
 * 「接入源码」命令（不预设规格，第一步让操作员自己选），以及「源码规格」抽屉里
 * 每行的「上传到此规格」（预设并**锁定**该规格，且沿用抽屉当时的环境）。两处都
 * 是宿主，本组件不自己决定环境以外的范围。
 */
import { useEffect, useMemo, useRef, useState, type ChangeEvent } from "react";
import type {
  AppPublishEnvironment,
  AppResponse,
  AppSourceSpecResponse,
  BindAppSourceSpecSourceRequest,
  SdkworkDeployAppClient,
} from "@sdkwork/deployments-pc-console-core/sdk";
import type { SdkworkDriveAppClient } from "@sdkwork/deployments-pc-console-core/sdk";
import type { DeploymentsLocale } from "@sdkwork/deployments-pc-commons";
import { BookOpen, FolderTree, GitBranch, HardDrive, Layers, Upload, type LucideIcon } from "lucide-react";
import { publishingTranslator, type PublishingTranslator } from "../i18n.ts";
import {
  SOURCE_BINDING_LABEL_KEYS,
  resolveAppEnvironment,
} from "../service/app-source-spec-routing.ts";
import {
  SOURCE_SPEC_TEMPLATES,
  planSourceSpecFromTemplate,
  type SourceSpecTemplate,
} from "../service/source-spec-templates.ts";
import { DriveNodePickerDialog, type DriveNodeSelection } from "./DriveNodePickerDialog.tsx";
import {
  DEPLOY_PACKAGE_TYPE_OPTIONS,
  createDeployAppOperationsService,
  type DeployAppOperationsService,
  type DeployCodeSource,
  type DeployUploadProgress,
} from "../service/deploy-app-operations.ts";
import css from "./create-deploy-app.module.css";

export interface UploadSourceDialogProps {
  readonly deployClient: SdkworkDeployAppClient
  readonly driveClient: SdkworkDriveAppClient
  readonly locale: DeploymentsLocale
  /** 目标应用：两种动作都必须挂在已存在的应用上（规格与 Drive 锚点都来自它）。 */
  readonly app: AppResponse
  /**
   * 规格的读取范围。缺省按应用自己的 `defaultEnvironment` 解析。
   *
   * 从「源码规格」抽屉进来时**必须原样传回抽屉当时的环境** —— 否则会读到另一个
   * 环境下的规格集合，预设的那条在列表里根本不存在。
   */
  readonly environment?: AppPublishEnvironment | undefined
  /**
   * 预设并锁定目标规格（从规格行的「上传到此规格」进入时给出）。
   *
   * 锁定而不是仅预选：入口本身就表达了这个意图，让它还能被改成另一条规格
   * 只会制造「点 A 绑到 B」的意外。
   */
  readonly presetSpecId?: string | undefined
  /** 应用尚无规格时「去声明源码规格」的落点；缺省则不渲染该引导。 */
  readonly onRequestSpecs?: (() => void) | undefined
  /** 成功回调（绑定成功与制品上传成功共用）：宿主据此提示并刷新。 */
  readonly onUploaded?: ((summary: string) => void) | undefined
  readonly onClose: () => void
  readonly theme?: ("light" | "dark") | undefined
  readonly size?: ("md" | "lg") | undefined
  /** 注入服务实例（测试用）；缺省按 client 构造。 */
  readonly service?: DeployAppOperationsService | undefined
}

/** 顶层动作。`attach` 对齐契约，`artifact` 是保留的应用级链路。 */
type SourceAction = "attach" | "artifact"

/** 契约 `BindAppSourceSpecSourceRequest["source"]` 的两个分支。 */
type SourceKind = "DRIVE_DIRECTORY" | "KNOWLEDGEBASE_WIKI"

const SOURCE_ACTIONS = ["attach", "artifact"] as const satisfies readonly SourceAction[]

const SOURCE_ACTION_LABEL_KEYS = {
  attach: "sourceActionBind",
  artifact: "sourceActionArtifact",
} as const

const SOURCE_ACTION_HINT_KEYS = {
  attach: "sourceActionBindHint",
  artifact: "sourceActionArtifactHint",
} as const

/**
 * 顶层动作的图标。
 *
 * `Record<SourceAction, …>` 而不是可选映射：动作是联合类型，新增一支时这里
 * 必编译错，图标与文案一起补齐 —— 一个没有图标的选项会在同一排卡片里显得
 * 像是坏的，而不是像是「不需要图标」。
 */
const SOURCE_ACTION_ICONS: Readonly<Record<SourceAction, LucideIcon>> = {
  attach: Layers,
  artifact: Upload,
}

/** 来源类型 → 文案键。`Record<SourceKind, …>` ⇒ 契约新增分支时这里必编译错。 */
const SOURCE_KIND_LABEL_KEYS: Readonly<Record<SourceKind, "sourceKindDrive" | "sourceKindWiki">> = {
  DRIVE_DIRECTORY: "sourceKindDrive",
  KNOWLEDGEBASE_WIKI: "sourceKindWiki",
}

const SOURCE_KIND_HINT_KEYS: Readonly<Record<SourceKind, "sourceKindDriveHint" | "sourceKindWikiHint">> = {
  DRIVE_DIRECTORY: "sourceKindDriveHint",
  KNOWLEDGEBASE_WIKI: "sourceKindWikiHint",
}

/** 来源类型的图标；理由同 `SOURCE_ACTION_ICONS`。 */
const SOURCE_KIND_ICONS: Readonly<Record<SourceKind, LucideIcon>> = {
  DRIVE_DIRECTORY: FolderTree,
  KNOWLEDGEBASE_WIKI: BookOpen,
}

const SOURCE_KINDS = ["DRIVE_DIRECTORY", "KNOWLEDGEBASE_WIKI"] as const satisfies readonly SourceKind[]

const GIT_PROVIDERS = [
  { value: "GITHUB", label: "GitHub" },
  { value: "GITEE", label: "Gitee" },
  { value: "GITLAB", label: "GitLab" },
  { value: "SELF_HOSTED", label: "Self-hosted" },
] as const;

type GitProvider = (typeof GIT_PROVIDERS)[number]["value"];

/** 「全部后缀」哨兵值：`accept` 里不能写 `*`，但可以什么都不写。 */
const ZIP_ACCEPT = ".zip,application/zip,application/x-zip-compressed";

/**
 * A clone URL the console is willing to submit.
 *
 * Deliberately the loosest shape that is still a URL: the contract stores an
 * HTTPS clone URL, and the console cannot validate a host on the user's behalf.
 * Anything without a scheme would be stored verbatim and fail at clone time,
 * so it is refused here instead.
 */
const GIT_CLONE_URL_PATTERN = /^https?:\/\/\S+$/i

/**
 * A publication UUID, wherever it sits in the pasted text.
 *
 * The knowledgebase has no "list publications" endpoint the console can call, so
 * the field cannot be a picker — but it also must not be a transcription
 * exercise. A publication is quoted in the knowledgebase UI as a link, and
 * operators copy that link, so the value is *extracted* from whatever was
 * pasted rather than required to be a bare UUID. `[0-9a-f]` with the canonical
 * 8-4-4-4-12 grouping is the whole rule; the case is normalised so two spellings
 * of one UUID never look like two publications.
 */
const PUBLICATION_UUID_PATTERN = /[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}/i

/**
 * The publication UUID inside whatever the operator pasted, or `undefined`.
 *
 * Exported so the extraction rule stays unit-testable without a renderer — it is
 * the one piece of this dialog that parses free text.
 */
export function extractPublicationUuid(raw: string): string | undefined {
  const match = PUBLICATION_UUID_PATTERN.exec(raw.trim())
  // `RegExp.exec` hands back `null`, not `undefined` — the two are different
  // enough here that the narrow has to name the right one.
  return match === null ? undefined : match[0].toLowerCase()
}

/**
 * Provider implied by a clone URL's host, for the hosts the console names.
 *
 * Returns `undefined` rather than `SELF_HOSTED` for an unknown host: the field is
 * a choice the operator made, and silently rewriting it to "Self-hosted" would
 * throw that choice away. Only a host the console positively recognises
 * overwrites it.
 */
export function inferGitProvider(rawUrl: string): GitProvider | undefined {
  const url = rawUrl.trim().toLowerCase()
  if (url === "") return undefined
  if (url.includes("github.com")) return "GITHUB"
  if (url.includes("gitee.com")) return "GITEE"
  if (url.includes("gitlab")) return "GITLAB"
  return undefined
}

/**
 * A repository key suggested by a clone URL — its last path (or `:`-separated)
 * segment with the `.git` suffix and any illegal characters removed.
 *
 * Only ever a *suggestion*: the caller stops applying it the moment the operator
 * edits the key themselves, because the key is an application-scoped identifier
 * that may deliberately differ from the repository's name.
 */
export function suggestRepoKey(rawUrl: string): string | undefined {
  const match = /(?:[:/])([^/:\s]+?)(?:\.git)?\/?$/.exec(rawUrl.trim())
  const segment = match?.[1]
  if (segment === undefined || segment === "") return undefined
  const key = segment.toLowerCase().replace(/[^a-z0-9._-]+/g, "-").replace(/^-+|-+$/g, "")
  return key === "" ? undefined : key
}

export function UploadSourceDialog({
  deployClient,
  driveClient,
  locale,
  app,
  environment: injectedEnvironment,
  presetSpecId,
  onRequestSpecs,
  onUploaded,
  onClose,
  theme = "light",
  size = "lg",
  service: injectedService,
}: UploadSourceDialogProps) {
  const t = useMemo(() => publishingTranslator(locale), [locale])
  const service = useMemo(
    () => injectedService ?? createDeployAppOperationsService({ deployClient, driveClient }),
    [injectedService, deployClient, driveClient],
  )
  const environment = useMemo(
    () => injectedEnvironment ?? resolveAppEnvironment(app),
    [injectedEnvironment, app],
  )

  const [action, setAction] = useState<SourceAction>("attach")

  /* ---- 绑定源码来源 ---- */
  const [specs, setSpecs] = useState<readonly AppSourceSpecResponse[]>()
  const [specsLoading, setSpecsLoading] = useState(false)
  const [specsError, setSpecsError] = useState<string>()
  const [refreshSpecs, setRefreshSpecs] = useState(0)
  const [specId, setSpecId] = useState(presetSpecId ?? "")
  const [sourceKind, setSourceKind] = useState<SourceKind>("DRIVE_DIRECTORY")
  const [bindFolder, setBindFolder] = useState<DriveNodeSelection>()
  const [bindUseSpaceRoot, setBindUseSpaceRoot] = useState(false)
  const [bindContentMode, setBindContentMode] = useState<"LIVE_TREE" | "ATOMIC_GENERATION">("LIVE_TREE")
  const [bindPickerOpen, setBindPickerOpen] = useState(false)
  const [publicationUuid, setPublicationUuid] = useState("")

  /* ---- 上传制品 ---- */
  const [source, setSource] = useState<DeployCodeSource>("local")
  const [packageType, setPackageType] = useState<number>(DEPLOY_PACKAGE_TYPE_OPTIONS[0]?.value ?? 1)
  const [file, setFile] = useState<File>()
  const [checksum, setChecksum] = useState<string>()
  const [progress, setProgress] = useState<DeployUploadProgress>()
  const [artifactNode, setArtifactNode] = useState<DriveNodeSelection>()
  const [artifactPickerOpen, setArtifactPickerOpen] = useState(false)

  /* ---- 共用 ---- */
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string>()
  const [notice, setNotice] = useState<string>()
  // React 19 requires an explicit initial value for `useRef`, so the "no
  // controller in flight" state is `undefined` rather than an omitted argument.
  const abortRef = useRef<AbortController | undefined>(undefined)
  const fileInputRef = useRef<HTMLInputElement>(null)

  // Git form
  const [repoKey, setRepoKey] = useState("")
  const [repoUrl, setRepoUrl] = useState("")
  const [repoProvider, setRepoProvider] = useState<GitProvider>("GITHUB")
  const [defaultBranch, setDefaultBranch] = useState("")
  const [credentialRef, setCredentialRef] = useState("")
  /**
   * 操作员是否亲手改过仓库标识。
   *
   * 一次性的开关而不是「值是否等于建议值」：后者会在用户把标识改回建议值的
   * 那一刻重新开始自动派生，于是下一次改 URL 又会覆盖他刚写下的东西。
   */
  const [repoKeyTouched, setRepoKeyTouched] = useState(false)

  /** 正在声明的模板 id —— 空态内联建规格时锁住整块网格，避免并发声明两份。 */
  const [templateBusy, setTemplateBusy] = useState<string>()

  /** 网盘回读进度（0–100）。上传进度是另一条，回读同样需要可见性。 */
  const [driveReadPercent, setDriveReadPercent] = useState<number>()

  const pickerOpen = bindPickerOpen || artifactPickerOpen

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      // 网盘选择器自己处理 Esc；两层同时监听会让 Esc 一次关掉两个面板。
      if (event.key === "Escape" && !busy && !pickerOpen) onClose()
    }
    document.addEventListener("keydown", onKeyDown)
    const previousOverflow = document.body.style.overflow
    document.body.style.overflow = "hidden"
    return () => {
      document.removeEventListener("keydown", onKeyDown)
      document.body.style.overflow = previousOverflow
    }
  }, [busy, onClose, pickerOpen])

  // 离开对话框时中断在飞的 Drive 上传，否则用户关掉面板后浏览器还在传。
  useEffect(() => () => { abortRef.current?.abort() }, [])

  /**
   * 规格清单只在「绑定源码来源」这一侧需要。
   *
   * 懒加载而不是挂载即取：只来传压缩包的操作员不该为一次列表请求等待，而且
   * 「上传制品」根本不读规格。切到该动作时（或绑定成功后重载时）才发请求。
   */
  useEffect(() => {
    if (action !== "attach") return
    let active = true
    setSpecsLoading(true)
    void service.listSourceSpecs(app.id, environment).then((items) => {
      if (active) {
        setSpecs(items)
        setSpecsError(undefined)
      }
    }).catch((cause) => {
      if (active) setSpecsError(t("sourceSpecsLoadFailed", { message: messageOf(cause) }))
    }).finally(() => {
      if (active) setSpecsLoading(false)
    })
    return () => { active = false }
  }, [action, app.id, environment, refreshSpecs, service, t])

  const sortedSpecs = useMemo(
    () => (specs === undefined ? undefined : [...specs].sort((left, right) => left.specKey.localeCompare(right.specKey))),
    [specs],
  )
  const selectedSpec = useMemo(
    () => specs?.find((spec) => spec.id === specId),
    [specId, specs],
  )
  const specLocked = presetSpecId !== undefined

  /**
   * 这份规格现在处在什么状态 —— 三条**不影响能否提交、但决定绑了以后是否真会服务**
   * 的事实。列表为空、状态停用、还没有声明服务哪一端，任意一条成立时「绑定成功」
   * 都不等于「开始服务」，所以它们必须出现在确认按钮之前。
   */
  const specNotes = (spec: AppSourceSpecResponse): readonly string[] => {
    const notes: string[] = []
    if (spec.status !== "ACTIVE") notes.push(t("sourceSpecDisabled"))
    if (spec.clientClassRoutes.length === 0) notes.push(t("sourceSpecNoRoute"))
    if (spec.sourceStatus !== "EMPTY") {
      notes.push(t("sourceSpecHasSource", { status: t(SOURCE_BINDING_LABEL_KEYS[spec.sourceStatus]) }))
    }
    return notes
  }

  const limitMiB = useMemo(
    () => DEPLOY_PACKAGE_TYPE_OPTIONS.find((option) => option.value === packageType)?.maxSizeMiB ?? 2048,
    [packageType],
  )

  /**
   * Wiki 发布 uuid 的解析结果。
   *
   * 两态要分开：**还没填**（不报错）与**填了但提不出 uuid**（报错并禁用提交）。
   * 少了第二档，「粘贴了一半」和「粘贴正确」在屏幕上长得一样，而前者此前会一路
   * 发到服务端才被拒。
   */
  const publicationParsed = useMemo(() => {
    const raw = publicationUuid.trim()
    const uuid = extractPublicationUuid(raw)
    return {
      uuid,
      invalid: raw !== "" && uuid === undefined,
    }
  }, [publicationUuid])

  /** Git 的克隆地址是否成形；只在 Git 分支参与提交判定。 */
  const gitUrlValid = GIT_CLONE_URL_PATTERN.test(repoUrl.trim())

  /**
   * 从模板声明一份源码规格，并把新建的那条选中。
   *
   * 「没有规格 ⇒ 只能退出去声明」是这条链路此前最大的断裂点：操作员被迫关掉当前
   * 面板、在另一个面板里完成声明、再回来重新走一遍绑定。规格与来源本来就是同一次
   * 意图的两半，这里把两半接上 —— 声明完直接落到「② 来源」，不必回头。
   */
  const declareFromTemplate = async (template: SourceSpecTemplate) => {
    setTemplateBusy(template.id)
    setError(undefined)
    setNotice(undefined)
    try {
      const request = planSourceSpecFromTemplate(
        template,
        environment,
        specs ?? [],
        t(template.labelKey),
      )
      const created = await service.createSourceSpec(app.id, request)
      setNotice(t("specTemplateCreated", { key: created.specKey }))
      // 直接选中新建的那条：接下来要绑的来源就属于它，让操作员再挑一次只会多一次出错机会。
      setSpecId(created.id)
      setRefreshSpecs((value) => value + 1)
    } catch (cause) {
      setError(t("specTemplateFailed", { message: messageOf(cause) }))
    } finally {
      setTemplateBusy(undefined)
    }
  }

  /**
   * 仓库地址联动：托管平台能认出来就填上，标识只在操作员没亲手写过时补建议。
   *
   * 这两件事都不改变提交的语义（请求体仍由操作员确认过的字段组成），只是把
   * 「从 URL 一眼可见」的信息替他填一次。
   */
  const onRepoUrlChange = (value: string) => {
    setRepoUrl(value)
    setError(undefined)
    if (value.trim() === "") return
    const provider = inferGitProvider(value)
    if (provider !== undefined) setRepoProvider(provider)
    if (!repoKeyTouched) {
      const suggestion = suggestRepoKey(value)
      if (suggestion !== undefined) setRepoKey(suggestion)
    }
  }

  const onPickFile = async (event: ChangeEvent<HTMLInputElement>) => {
    const next = event.target.files?.[0]
    // 同一个文件连续选两次不会触发 change，必须清空 value。
    event.target.value = ""
    if (next === undefined) return
    if (!/\.zip$/i.test(next.name)) {
      setError(t("uploadPackageRejected"))
      return
    }
    if (next.size > limitMiB * 1024 * 1024) {
      setError(t("uploadPackageTooLarge", { limit: String(limitMiB) }))
      return
    }
    setError(undefined)
    setFile(next)
    // 摘要与上传是两个阶段，先算完再允许提交 —— 上传中途才发现读不了文件
    // 会让用户在进度条上白等一次。
    setChecksum(undefined)
    try {
      setChecksum(await service.archiveChecksum(next))
    } catch (cause) {
      setError(errorText(cause, t, "uploadFailed"))
    }
  }

  /**
   * 「上传制品」的网盘选择器：要的是**文件**。目录没有单一字节流，不能进上传
   * 链路 —— 这里显式拒绝并让用户重新选，而不是替它伪造一个压缩包名。
   */
  const onArtifactNodeSelected = (selection: DriveNodeSelection) => {
    setArtifactPickerOpen(false)
    setProgress(undefined)
    setNotice(undefined)
    if (selection.nodeKind === "folder") {
      setArtifactNode(undefined)
      setError(t("drivePickerFolderNotPackage"))
      return
    }
    setArtifactNode(selection)
    setError(undefined)
  }

  /**
   * 「绑定源码来源」的网盘选择器：要的是**目录**，与上一条正好相反。
   *
   * 选中文件时清掉上一次的选择 —— 否则「再确认一次」会把上一次那个目录绑上去，
   * 而屏幕上显示的是刚点的文件。
   */
  const onBindFolderSelected = (selection: DriveNodeSelection) => {
    setBindPickerOpen(false)
    setNotice(undefined)
    if (selection.nodeKind === "file") {
      setBindFolder(undefined)
      setError(t("sourceSpecsBindNeedsFolder"))
      return
    }
    setBindFolder(selection)
    setUseSpaceRootLater(false)
    setError(undefined)
  }

  // `setUseSpaceRoot` 只在目录选择之后有意义：根目录与某个子目录是互斥的两个选项，
  // 换目录时回到「就是这个目录」，否则上一次勾的「用空间根」会静默作用到新目录上。
  const setUseSpaceRootLater = (value: boolean) => { setBindUseSpaceRoot(value) }

  const activeArchive = source === "local"
    ? file === undefined
      ? undefined
      : { name: file.name, size: file.size }
    : artifactNode === undefined
      ? undefined
      : { name: artifactNode.nodeName, size: artifactNode.contentLength }

  const canSubmit = action === "attach"
    ? specId !== "" && (sourceKind === "DRIVE_DIRECTORY"
      ? bindFolder !== undefined
      // 只认能解析出 uuid 的输入。此前「填了任何非空文本」就算可提交，一段
      // 认不出的文本会一路发到服务端才被拒，而屏幕上没有任何字段说明原因。
      : publicationParsed.uuid !== undefined)
    : source === "local"
      ? file !== undefined && checksum !== undefined
      : source === "git"
        ? repoKey.trim() !== "" && gitUrlValid
        : artifactNode !== undefined

  const abort = () => {
    abortRef.current?.abort()
  }

  const submit = async () => {
    setBusy(true)
    setError(undefined)
    setNotice(undefined)
    try {
      if (action === "attach") {
        const spec = selectedSpec
        if (spec === undefined) {
          setError(t("sourceSpecRequired"))
          setBusy(false)
          return
        }
        // 联合的两支在这里收窄：`DRIVE_DIRECTORY` 需要目录与内容模式，
        // `KNOWLEDGEBASE_WIKI` 只需要一个发布 uuid。分支穷尽由 `sourceKind`
        // 的联合类型保证，不是靠运行时兜底。
        const wikiUuid = publicationParsed.uuid
        if (sourceKind === "KNOWLEDGEBASE_WIKI" && wikiUuid === undefined) {
          setError(t("sourceWikiInvalid"))
          setBusy(false)
          return
        }
        const value: BindAppSourceSpecSourceRequest["source"] = sourceKind === "DRIVE_DIRECTORY"
          ? {
            type: "DRIVE_DIRECTORY",
            websiteSpaceId: (bindFolder as DriveNodeSelection).spaceId,
            root: bindUseSpaceRoot
              ? { mode: "SPACE_ROOT" }
              : { mode: "FOLDER", folderNodeId: (bindFolder as DriveNodeSelection).nodeId },
            contentMode: bindContentMode,
          }
          // 断言成立：上面的早退已经排除了 `undefined`，而 `canSubmit` 也不会
          // 放一个解析不出的输入进来 —— 两道都在，是为了让「点得动」与「发得出」
          // 永远指着同一个条件。
          : { type: "KNOWLEDGEBASE_WIKI", publicationUuid: wikiUuid as string }
        const updated = await service.bindSourceSpecSource(app.id, spec.id, value)
        const summary = t("sourceSpecsBindSucceeded", { key: updated.specKey })
        setNotice(summary)
        onUploaded?.(summary)
        // 绑完之后这一条的状态（已绑定 / 每端链条）变了：重读一次，好让操作员
        // 顺手接着绑下一条时看到的是真状态，而不是打开面板那一刻的快照。
        setRefreshSpecs((value) => value + 1)
        setBusy(false)
        return
      }

      if (source === "git") {
        const repository = await service.connectGitSource(app.id, {
          repoKey: repoKey.trim(),
          repoProvider,
          repoUrl: repoUrl.trim(),
          ...(defaultBranch.trim() === "" ? {} : { defaultBranch: defaultBranch.trim() }),
          ...(credentialRef.trim() === "" ? {} : { credentialSecretRef: credentialRef.trim() }),
        })
        const summary = t("uploadGitSucceeded", { repoKey: repository.repoKey })
        setNotice(summary)
        onUploaded?.(summary)
        setBusy(false)
        return
      }

      if (source === "drive") {
        // 网盘里的文件已经落在 Drive 上，但没有 `deploy_artifact` 记录 ——
        // 制品登记才是发布能消费它的前提。这里读回字节再走同一条 publisher
        // 链路，保证制品与本地压缩包形态完全一致。
        const node = artifactNode as DriveNodeSelection
        if (node.contentLength > limitMiB * 1024 * 1024) {
          setError(t("uploadPackageTooLarge", { limit: String(limitMiB) }))
          setBusy(false)
          return
        }
        setDriveReadPercent(0)
        const bytes = await downloadDriveNode(driveClient, node, (percent) => { setDriveReadPercent(percent) })
        const driveFile = new File([bytes], node.nodeName, {
          type: node.contentType ?? "application/octet-stream",
        })
        const digest = await service.archiveChecksum(driveFile)
        abortRef.current = new AbortController()
        const result = await service.uploadCodeFromArchive({
          appId: app.id,
          packageType,
          archive: {
            file: driveFile,
            fileName: node.nodeName,
            contentType: node.contentType ?? "application/octet-stream",
            checksumSha256: digest,
          },
          signal: abortRef.current.signal,
          onProgress: setProgress,
        })
        const summary = t("uploadSucceeded", { artifactId: result.artifactId })
        setNotice(summary)
        onUploaded?.(summary)
        setBusy(false)
        return
      }

      const localFile = file as File
      const digest = checksum as string
      abortRef.current = new AbortController()
      const result = await service.uploadCodeFromArchive({
        appId: app.id,
        packageType,
        archive: {
          file: localFile,
          fileName: localFile.name,
          contentType: localFile.type || "application/zip",
          checksumSha256: digest,
        },
        signal: abortRef.current.signal,
        onProgress: setProgress,
      })
      const summary = t("uploadSucceeded", { artifactId: result.artifactId })
      setNotice(summary)
      onUploaded?.(summary)
      setBusy(false)
    } catch (cause) {
      setError(action === "attach"
        ? t("sourceSpecsBindFailed", { message: messageOf(cause) })
        : errorText(cause, t, "uploadFailed"))
      setBusy(false)
    } finally {
      abortRef.current = undefined
      // 回读进度属于「正在进行的那一次提交」：无论成功、失败还是被中断，都不该
      // 留在屏幕上被读成「还在读」。
      setDriveReadPercent(undefined)
    }
  }

  const percent = progress === undefined || progress.totalBytes === 0
    ? undefined
    : Math.min(100, Math.round((progress.uploadedBytes / progress.totalBytes) * 100))

  const submitLabel = busy
    ? t(action === "attach" ? "saving" : "uploadInProgress")
    : action === "attach"
      ? t("sourceSpecsBindConfirm")
      : source === "git"
        ? t("uploadGitConnect")
        : t("uploadConfirm")

  return (
    <div
      className={css.drawerRoot}
      data-theme={theme}
      role="presentation"
      onMouseDown={(event) => { if (event.target === event.currentTarget && !busy) onClose() }}
    >
      <div
        className={`${css.drawerPanel}${size === "lg" ? ` ${css.drawerPanelLg}` : ""}`}
        role="dialog"
        aria-modal="true"
        aria-label={t("uploadCodeTitle", { name: app.name })}
      >
        <header className={css.header}>
          <div className={css.headerText}>
            <h2>{t("uploadCodeTitle", { name: app.name })}</h2>
            <p>{t("uploadCodeDescription")}</p>
          </div>
          <button type="button" className={css.closeButton} title={t("close")} aria-label={t("close")} onClick={onClose}>
            ×
          </button>
        </header>

        <div className={css.body}>
          {/* 顶层动作。放在最前：它决定下面整个表单的形状，是「接入源码」这条命令
              真正的第一步 —— 是给某份规格接来源，还是给应用添一份制品。 */}
          <div className={css.radioGroup} role="radiogroup" aria-label={t("sourceActionLabel")}>
            {SOURCE_ACTIONS.map((value) => {
              const ActionIcon = SOURCE_ACTION_ICONS[value]
              return (
                <label key={value} className={css.radioRow} data-selected={action === value}>
                  <input
                    type="radio"
                    name="upload-action"
                    checked={action === value}
                    disabled={busy}
                    onChange={() => { setAction(value); setError(undefined); setNotice(undefined) }}
                  />
                  <span className={css.radioLabel}>
                    {/* 图标与标题同一行：卡片的「这是什么」要一眼可辨；说明留到
                        下一行，扫描顺序才是「标题 → 解释」。 */}
                    <span className={css.radioTitleRow}>
                      <ActionIcon aria-hidden="true" size={15} />
                      <strong>{t(SOURCE_ACTION_LABEL_KEYS[value])}</strong>
                    </span>
                    <small>{t(SOURCE_ACTION_HINT_KEYS[value])}</small>
                  </span>
                </label>
              )
            })}
          </div>

          {action === "attach"
            ? (
              <>
                {/* ① 目标规格 */}
                <section className={css.detailSection} data-step="spec">
                  <h3 className={css.detailSectionTitle}>{t("sourceSpecStepTitle")}</h3>
                  {specsError !== undefined && (
                    <div className={css.errorBanner} role="alert">{specsError}</div>
                  )}
                  {specsLoading && specs === undefined
                    ? <span className={css.fieldHint}>{t("sourceSpecLoading")}</span>
                    : sortedSpecs === undefined || sortedSpecs.length === 0
                      ? (
                        <div className={css.field}>
                          <span className={css.fieldLabel}>{t("sourceSpecNone")}</span>
                          <span className={css.fieldHint}>{t("sourceSpecNoneHint")}</span>

                          {/* 声明入口就放在这里，而不是请操作员退出去另一个面板：
                              规格与来源是同一次意图的两半，拆成两个面板意味着他要
                              关掉当前面板、重新找到这个应用、再重新打开这条命令。
                              模板替他回答「规格的九个字段该填什么」，手动声明留作
                              少数确实需要逐字段控制的出口。 */}
                          <div className={css.templateSection}>
                            <h4 className={css.templateTitle}>{t("specTemplateTitle")}</h4>
                            <p className={css.templateHint}>{t("specTemplateHint")}</p>
                            <div className={css.templateGrid}>
                              {SOURCE_SPEC_TEMPLATES.map((template) => (
                                <button
                                  key={template.id}
                                  type="button"
                                  className={css.templateCard}
                                  data-template={template.id}
                                  disabled={busy || templateBusy !== undefined}
                                  onClick={() => { void declareFromTemplate(template) }}
                                >
                                  <span className={css.templateCardName}>{t(template.labelKey)}</span>
                                  <span className={css.templateCardHint}>{t(template.hintKey)}</span>
                                </button>
                              ))}
                            </div>
                            {templateBusy !== undefined && (
                              <span className={css.fieldHint} role="status">{t("specTemplateCreating")}</span>
                            )}
                          </div>

                          {onRequestSpecs !== undefined && (
                            <div className={css.mediaFileRow}>
                              <button
                                type="button"
                                className={css.secondaryButton}
                                disabled={busy || templateBusy !== undefined}
                                onClick={onRequestSpecs}
                              >
                                {t("sourceSpecNoneAction")}
                              </button>
                            </div>
                          )}
                        </div>
                      )
                      : (
                        <>
                          {specLocked
                            ? (
                              <div className={css.field}>
                                <span className={css.fieldLabel}>{t("sourceSpecLabel")}</span>
                                <span className={css.specRowTitle}>
                                  <strong>{selectedSpec?.label ?? presetSpecId}</strong>
                                  {selectedSpec !== undefined && (
                                    <code className={css.specKey}>{selectedSpec.specKey}</code>
                                  )}
                                  <span className={css.specBadge}>{t("sourceSpecLocked")}</span>
                                </span>
                              </div>
                            )
                            : (
                              <label className={css.field}>
                                <span className={css.fieldLabel}>{t("sourceSpecLabel")}</span>
                                <select
                                  className={css.select}
                                  value={specId}
                                  disabled={busy}
                                  aria-label={t("sourceSpecLabel")}
                                  onChange={(event) => { setSpecId(event.target.value); setError(undefined) }}
                                >
                                  <option value="">{t("sourceSpecPlaceholder")}</option>
                                  {sortedSpecs.map((spec) => (
                                    <option key={spec.id} value={spec.id}>{spec.specKey} · {spec.label}</option>
                                  ))}
                                </select>
                              </label>
                            )}
                          <span className={css.fieldHint}>{t("sourceSpecHint")}</span>
                          {selectedSpec !== undefined && specNotes(selectedSpec).map((note) => (
                            <span key={note} className={css.fieldHint}>{note}</span>
                          ))}
                        </>
                      )}
                </section>

                {/* ② 来源类型。规格未定时不渲染：一个还没有归属的选择没有意义，
                    而且它会让「先选规格」这个顺序看起来是可选的。 */}
                {selectedSpec !== undefined && (
                  <section className={css.detailSection} data-step="source">
                    <h3 className={css.detailSectionTitle}>{t("sourceKindStepTitle")}</h3>
                    <div className={css.radioGroup} role="radiogroup" aria-label={t("sourceKindLabel")}>
                      {SOURCE_KINDS.map((value) => {
                        const KindIcon = SOURCE_KIND_ICONS[value]
                        return (
                          <label key={value} className={css.radioRow} data-selected={sourceKind === value}>
                            <input
                              type="radio"
                              name="upload-source-kind"
                              checked={sourceKind === value}
                              disabled={busy}
                              onChange={() => { setSourceKind(value); setError(undefined) }}
                            />
                            <span className={css.radioLabel}>
                              <span className={css.radioTitleRow}>
                                <KindIcon aria-hidden="true" size={15} />
                                <strong>{t(SOURCE_KIND_LABEL_KEYS[value])}</strong>
                              </span>
                              <small>{t(SOURCE_KIND_HINT_KEYS[value])}</small>
                            </span>
                          </label>
                        )
                      })}
                    </div>

                    {sourceKind === "DRIVE_DIRECTORY"
                      ? (
                        <>
                          <div className={css.field}>
                            <div className={css.mediaFileRow}>
                              <button
                                type="button"
                                className={css.secondaryButton}
                                disabled={busy}
                                onClick={() => { setBindPickerOpen(true); setError(undefined) }}
                              >
                                {t("sourceSpecsBindPick")}
                              </button>
                              {bindFolder !== undefined && (
                                <span className={css.mediaFileName}>
                                  {bindUseSpaceRoot
                                    ? `${bindFolder.spaceName} /`
                                    : bindFolder.displayPath}
                                </span>
                              )}
                            </div>
                          </div>
                          {/* 根目录没有 `nodeId`（选择器只在进了空间之后才能确认选中），
                              所以「用该空间的根」是一个显式开关，而不是让用户去猜怎么点出根。 */}
                          <label className={css.specToggle} title={t("sourceSpecsBindUseSpaceRoot")}>
                            <input
                              type="checkbox"
                              checked={bindUseSpaceRoot}
                              disabled={bindFolder === undefined || busy}
                              onChange={(event) => { setBindUseSpaceRoot(event.target.checked) }}
                            />
                            <span>{t("sourceSpecsBindUseSpaceRoot")}</span>
                          </label>
                          <label className={css.field}>
                            <span className={css.fieldLabel}>{t("sourceSpecsBindContentMode")}</span>
                            <select
                              className={css.select}
                              value={bindContentMode}
                              disabled={busy}
                              aria-label={t("sourceSpecsBindContentMode")}
                              onChange={(event) => {
                                setBindContentMode(event.target.value as "LIVE_TREE" | "ATOMIC_GENERATION")
                              }}
                            >
                              <option value="LIVE_TREE">{t("sourceSpecsBindLive")}</option>
                              <option value="ATOMIC_GENERATION">{t("sourceSpecsBindAtomic")}</option>
                            </select>
                          </label>
                        </>
                      )
                      : (
                        <label className={css.field}>
                          <span className={css.fieldLabel}>{t("sourceWikiPublication")}</span>
                          <input
                            className={css.input}
                            value={publicationUuid}
                            placeholder={t("sourceWikiPublicationPlaceholder")}
                            disabled={busy}
                            aria-invalid={publicationParsed.invalid}
                            onChange={(event) => { setPublicationUuid(event.target.value); setError(undefined) }}
                          />
                          <span className={css.fieldHint}>{t("sourceWikiPublicationHint")}</span>
                          {/* 识别出来了就把**将引用的那个 uuid** 回显出来。粘贴链接是
                              常态，回显能让「我粘的是不是我想要的那一版」当场可见，
                              而不是等绑定成功之后才从回执里猜。 */}
                          {publicationParsed.uuid !== undefined && (
                            <span className={css.wikiResolved}>
                              {t("sourceWikiResolved", { uuid: publicationParsed.uuid })}
                            </span>
                          )}
                          {publicationParsed.invalid && (
                            <span className={css.fieldError} role="alert">{t("sourceWikiInvalid")}</span>
                          )}
                        </label>
                      )}
                  </section>
                )}
              </>
            )
            : (
              <>
                <div className={css.radioGroup} role="radiogroup" aria-label={t("uploadSourceTypeLabel")}>
                  {(["local", "git", "drive"] as const).map((value) => {
                    const SourceIcon = SOURCE_ICONS[value]
                    return (
                      <label key={value} className={css.radioRow} data-selected={source === value}>
                        <input
                          type="radio"
                          name="upload-source"
                          checked={source === value}
                          disabled={busy}
                          onChange={() => { setSource(value); setError(undefined) }}
                        />
                        <span className={css.radioLabel}>
                          <span className={css.radioTitleRow}>
                            <SourceIcon aria-hidden="true" size={15} />
                            <strong>{t(SOURCE_LABEL_KEYS[value])}</strong>
                          </span>
                          <small>{t(SOURCE_HINT_KEYS[value])}</small>
                        </span>
                      </label>
                    )
                  })}
                </div>

                {source !== "git" && (
                  <div className={css.field}>
                    <span className={css.fieldLabel}>{t("uploadPackageType")}</span>
                    <select
                      className={css.select}
                      value={String(packageType)}
                      disabled={busy}
                      onChange={(event) => { setPackageType(Number(event.target.value)) }}
                    >
                      {DEPLOY_PACKAGE_TYPE_OPTIONS.map((option) => (
                        <option key={option.value} value={option.value}>{t(option.labelKey)}</option>
                      ))}
                    </select>
                    <span className={css.fieldHint}>{t("uploadPackageTypeHint")}</span>
                  </div>
                )}

                {source === "local" && (
                  <div className={css.field}>
                    <span className={css.fieldLabel}>{t("uploadPackageFile")}</span>
                    <div className={css.mediaFileRow}>
                      <button
                        type="button"
                        className={css.secondaryButton}
                        disabled={busy}
                        onClick={() => { fileInputRef.current?.click() }}
                      >
                        {t("uploadPackageChoose")}
                      </button>
                      {activeArchive !== undefined && (
                        <span className={css.mediaFileName}>
                          {activeArchive.name} · {formatBytes(activeArchive.size)}
                        </span>
                      )}
                    </div>
                    <input
                      ref={fileInputRef}
                      className={css.fileInputHidden}
                      type="file"
                      accept={ZIP_ACCEPT}
                      onChange={(event) => { void onPickFile(event) }}
                    />
                    <span className={css.fieldHint}>
                      {t("uploadPackageHint", { limit: String(limitMiB) })}
                    </span>
                    {checksum !== undefined && (
                      <span className={css.fieldHint}>
                        {t("uploadChecksum")}: <code>{checksum.slice(0, 16)}…</code>
                      </span>
                    )}
                  </div>
                )}

                {source === "git" && (
                  <>
                    {/* 顺序是「先给地址，再让它派生其余」：地址是唯一无法从别的字段
                        推出来的信息，标识与托管平台都能从它得到建议。此前五个字段平铺
                        且顺序相反（标识在最前），于是操作员要先想一个标识、再粘地址，
                        而地址已经含着他想要的那个名字。 */}
                    <section className={css.fieldSection}>
                      <h4 className={css.fieldSectionTitle}>{t("uploadGitSectionRepo")}</h4>

                      <div className={css.field}>
                        <span className={css.fieldLabel}>{t("uploadGitRepoUrl")}</span>
                        <input
                          className={css.input}
                          value={repoUrl}
                          placeholder={t("uploadGitRepoUrlPlaceholder")}
                          disabled={busy}
                          aria-invalid={repoUrl.trim() !== "" && !gitUrlValid}
                          onChange={(event) => { onRepoUrlChange(event.target.value) }}
                        />
                        <span className={css.fieldHint}>{t("uploadGitRepoUrlHint")}</span>
                        {repoUrl.trim() !== "" && !gitUrlValid && (
                          <span className={css.fieldError} role="alert">{t("uploadGitUrlInvalid")}</span>
                        )}
                      </div>

                      <div className={css.field}>
                        <span className={css.fieldLabel}>{t("uploadGitRepoKey")}</span>
                        <input
                          className={css.input}
                          value={repoKey}
                          placeholder={t("uploadGitRepoKeyPlaceholder")}
                          disabled={busy}
                          onChange={(event) => { setRepoKey(event.target.value); setRepoKeyTouched(true) }}
                        />
                        <span className={css.fieldHint}>{t("uploadGitRepoKeyHint")}</span>
                      </div>

                      <div className={css.field}>
                        <span className={css.fieldLabel}>{t("uploadGitProvider")}</span>
                        <select
                          className={css.select}
                          value={repoProvider}
                          disabled={busy}
                          onChange={(event) => { setRepoProvider(event.target.value as GitProvider) }}
                        >
                          {GIT_PROVIDERS.map((provider) => (
                            <option key={provider.value} value={provider.value}>{provider.label}</option>
                          ))}
                        </select>
                        {inferGitProvider(repoUrl) !== undefined && (
                          <span className={css.fieldHint}>{t("uploadGitProviderInferred")}</span>
                        )}
                      </div>
                    </section>

                    <section className={css.fieldSection}>
                      <h4 className={css.fieldSectionTitle}>{t("uploadGitSectionBranch")}</h4>

                      <div className={css.field}>
                        <span className={css.fieldLabel}>{t("uploadGitDefaultBranch")}</span>
                        <input
                          className={css.input}
                          value={defaultBranch}
                          placeholder="main"
                          disabled={busy}
                          onChange={(event) => { setDefaultBranch(event.target.value) }}
                        />
                      </div>

                      <div className={css.field}>
                        <span className={css.fieldLabel}>{t("uploadGitCredentialRef")}</span>
                        <input
                          className={css.input}
                          value={credentialRef}
                          placeholder={t("uploadGitCredentialRefPlaceholder")}
                          disabled={busy}
                          onChange={(event) => { setCredentialRef(event.target.value) }}
                        />
                        <span className={css.fieldHint}>{t("uploadGitCredentialRefHint")}</span>
                      </div>
                    </section>
                  </>
                )}

                {source === "drive" && (
                  <div className={css.field}>
                    <span className={css.fieldLabel}>{t("uploadSourceDrive")}</span>
                    <div className={css.mediaFileRow}>
                      <button
                        type="button"
                        className={css.secondaryButton}
                        disabled={busy}
                        onClick={() => { setArtifactPickerOpen(true); setError(undefined) }}
                      >
                        {t("drivePickerTitle")}
                      </button>
                      {artifactNode !== undefined && (
                        <span className={css.mediaFileName}>
                          {artifactNode.displayPath}
                          {artifactNode.contentLength > 0 && ` · ${formatBytes(artifactNode.contentLength)}`}
                        </span>
                      )}
                    </div>
                    <span className={css.fieldHint}>{t("uploadSourceDriveHint")}</span>
                  </div>
                )}

                {/* 「不绑定规格」是这一侧最容易被误解的一点：制品上传成功与某个
                    规格开始对外服务是两件事，产物是应用级制品。写在动作区内而不是
                    只写在描述里，因为操作员真正读的是表单。 */}
                <span className={css.fieldHint}>{t("sourceActionArtifactScope")}</span>
              </>
            )}

          {driveReadPercent !== undefined && (
            <div className={css.field}>
              <span className={css.stepTitle}>{t("uploadDriveReading")}</span>
              <span className={css.uploadingText}>{driveReadPercent}%</span>
              <div
                className={css.progressTrack}
                role="progressbar"
                aria-valuenow={driveReadPercent}
                aria-valuemin={0}
                aria-valuemax={100}
              >
                <div className={css.progressBar} style={{ width: `${driveReadPercent}%` }} />
              </div>
            </div>
          )}

          {progress !== undefined && (
            <div className={css.field}>
              <span className={css.stepTitle}>{t("uploadInProgress")}</span>
              <span className={css.uploadingText}>
                {progress.stage}
                {percent !== undefined && ` · ${percent}%`}
                {progress.totalParts > 0 && ` · ${progress.uploadedParts}/${progress.totalParts}`}
              </span>
              {percent !== undefined && (
                <div className={css.progressTrack} role="progressbar" aria-valuenow={percent} aria-valuemin={0} aria-valuemax={100}>
                  <div className={css.progressBar} style={{ width: `${percent}%` }} />
                </div>
              )}
            </div>
          )}

          {/* 提交前复核。
              一次「接入源码」的结果不是立刻可见的（绑完要到下一次 compose 才生效，
              制品要经过 release 才能访问），所以「我到底提交了什么」必须在按下按钮
              **之前**是可读的。它只在真正可提交时出现 —— 一份填到一半的表单没有可
              复核的东西，提前显示只会把空值当成结论。 */}
          {!busy && canSubmit && (
            <section className={css.reviewSection} data-review="true">
              <h4 className={css.reviewTitle}>{t("sourceReviewTitle")}</h4>
              <dl className={css.reviewList}>
                {action === "attach"
                  ? (
                    <>
                      <div className={css.reviewRow}>
                        <dt className={css.reviewLabel}>{t("sourceSpecLabel")}</dt>
                        <dd className={css.reviewValue}>{selectedSpec?.specKey ?? specId}</dd>
                      </div>
                      <div className={css.reviewRow}>
                        <dt className={css.reviewLabel}>{t("sourceKindLabel")}</dt>
                        <dd className={css.reviewValue}>{t(SOURCE_KIND_LABEL_KEYS[sourceKind])}</dd>
                      </div>
                      <div className={css.reviewRow}>
                        <dt className={css.reviewLabel}>{t("sourceReviewLocation")}</dt>
                        <dd className={css.reviewValue}>
                          {sourceKind === "DRIVE_DIRECTORY"
                            ? bindFolder === undefined
                              ? "-"
                              : bindUseSpaceRoot
                                ? `${bindFolder.spaceName} /`
                                : bindFolder.displayPath
                            : publicationParsed.uuid ?? "-"}
                        </dd>
                      </div>
                    </>
                  )
                  : (
                    <>
                      <div className={css.reviewRow}>
                        <dt className={css.reviewLabel}>{t("uploadSourceTypeLabel")}</dt>
                        <dd className={css.reviewValue}>{t(SOURCE_LABEL_KEYS[source])}</dd>
                      </div>
                      {source !== "git" && (
                        <div className={css.reviewRow}>
                          <dt className={css.reviewLabel}>{t("uploadPackageType")}</dt>
                          <dd className={css.reviewValue}>
                            {t(DEPLOY_PACKAGE_TYPE_OPTIONS
                              .find((option) => option.value === packageType)?.labelKey ?? "uploadPackageType")}
                          </dd>
                        </div>
                      )}
                      <div className={css.reviewRow}>
                        <dt className={css.reviewLabel}>{t("uploadArtifactName")}</dt>
                        <dd className={css.reviewValue}>
                          {source === "git" ? repoUrl.trim() : activeArchive?.name ?? "-"}
                        </dd>
                      </div>
                    </>
                  )}
              </dl>
            </section>
          )}
        </div>

        <footer className={css.footer}>
          {error && <div className={css.errorBanner} role="alert">{error}</div>}
          {!error && notice && <div className={css.successBanner} role="status">{notice}</div>}
          {!error && !notice && <div className={css.footerSpacer} />}
          {busy && action === "artifact" && (source === "local" || source === "drive") && (
            <button type="button" className={css.secondaryButton} onClick={abort}>
              {t("cancel")}
            </button>
          )}
          <button type="button" className={css.secondaryButton} disabled={busy} onClick={onClose}>
            {t("close")}
          </button>
          <button
            type="button"
            className={css.primaryButton}
            disabled={busy || !canSubmit}
            onClick={() => { void submit() }}
          >
            {submitLabel}
          </button>
        </footer>
      </div>
      {bindPickerOpen && (
        <DriveNodePickerDialog
          driveClient={driveClient}
          locale={locale}
          theme={theme}
          onClose={() => { setBindPickerOpen(false) }}
          onSelected={onBindFolderSelected}
        />
      )}
      {artifactPickerOpen && (
        <DriveNodePickerDialog
          driveClient={driveClient}
          locale={locale}
          theme={theme}
          onClose={() => { setArtifactPickerOpen(false) }}
          onSelected={onArtifactNodeSelected}
        />
      )}
    </div>
  )
}

const SOURCE_LABEL_KEYS = {
  local: "uploadSourceLocal",
  git: "uploadSourceGit",
  drive: "uploadSourceDrive",
} as const;

const SOURCE_HINT_KEYS = {
  local: "uploadSourceLocalHint",
  git: "uploadSourceGitHint",
  drive: "uploadSourceDriveHint",
} as const;

/** 制品来源的图标；理由同 `SOURCE_ACTION_ICONS`。 */
const SOURCE_ICONS: Readonly<Record<DeployCodeSource, LucideIcon>> = {
  local: Upload,
  git: GitBranch,
  drive: HardDrive,
}

/**
 * Read a Drive node's bytes back into the browser.
 *
 * The Drive uploader only writes, so a reused archive is read back through the
 * generated Drive App SDK and handed to the same publisher as a `File`. That
 * keeps one artifact shape regardless of where the bytes came from, and it is
 * why the Drive branch still registers a fresh `deploy_artifact` — a Drive node
 * on its own is not something a release can consume.
 *
 * The read goes through `nodes.content.retrieve`, a same-origin Drive App API
 * operation that returns the standard `SdkWorkApiResponse` envelope with the
 * bytes in the requested encoding. This is deliberately *not* the download URL:
 * that operation hands back a presigned URL on the storage provider's origin,
 * and the PC console must not reach outside its own origin (see
 * `tests/architecture-boundary.test.ts`). The content endpoint is bounded, so an
 * archive larger than one response is read as a sequence of ranges.
 */
async function downloadDriveNode(
  driveClient: SdkworkDriveAppClient,
  node: DriveNodeSelection,
  onProgress?: (percent: number) => void,
): Promise<ArrayBuffer> {
  const chunks: Uint8Array[] = []
  let offset = 0
  let totalBytes: number | undefined

  for (;;) {
    const content = await driveClient.drive.nodes.content.retrieve(node.nodeId, {
      byteRangeStart: String(offset),
      byteRangeLength: DRIVE_CONTENT_CHUNK_BYTES,
      encoding: "base64",
    })
    const sizeBytes = Number(content.sizeBytes)
    if (Number.isFinite(sizeBytes)) {
      totalBytes = sizeBytes
    }
    const decoded = decodeDriveContentChunk(content.content)
    if (decoded.byteLength === 0) {
      break
    }
    chunks.push(decoded)
    offset += decoded.byteLength
    // 分片是**串行**的（4 MiB/片，包上限 2 GiB ⇒ 最多约 512 次往返），所以回读
    // 是一个可观察的长阶段：没有进度时界面在整个阶段看起来是卡住的，而这发生
    // 在「上传进度条」之前 —— 用户会以为点了没反应。
    if (onProgress !== undefined && totalBytes !== undefined && totalBytes > 0) {
      onProgress(Math.min(100, Math.round((offset / totalBytes) * 100)))
    }
    if (!content.hasMore) {
      break
    }
  }

  if (totalBytes !== undefined && offset !== totalBytes) {
    throw new Error(
      `Drive returned ${offset} of ${totalBytes} bytes for ${node.nodeName}.`,
    )
  }

  const merged = new Uint8Array(offset)
  let written = 0
  for (const chunk of chunks) {
    merged.set(chunk, written)
    written += chunk.byteLength
  }
  return merged.buffer
}

/**
 * Bytes requested per `nodes.content.retrieve` call.
 *
 * The Drive contract bounds a single content response, so a larger archive is
 * read as a sequence of ranges. This stays below that bound to leave room for
 * base64 expansion.
 */
const DRIVE_CONTENT_CHUNK_BYTES = 4 * 1024 * 1024

/**
 * Decode one `nodes.content.retrieve` payload into raw bytes.
 *
 * Exported so the chunk-decoding contract stays unit-testable without a live
 * Drive origin. `base64` is what the caller requests; the guard keeps a future
 * caller that asks for `utf8` from silently reading text bytes as binary.
 */
export function decodeDriveContentChunk(content: string): Uint8Array {
  const binary = atob(content)
  const bytes = new Uint8Array(binary.length)
  for (let index = 0; index < binary.length; index += 1) {
    bytes[index] = binary.charCodeAt(index)
  }
  return bytes
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`
  const units = ["KiB", "MiB", "GiB"]
  let value = bytes / 1024
  let unit = 0
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024
    unit += 1
  }
  return `${value.toFixed(value >= 10 ? 0 : 1)} ${units[unit]}`
}

function messageOf(cause: unknown): string {
  return cause instanceof Error && cause.message ? cause.message : String(cause)
}

/**
 * Map a failure to localised, actionable copy.
 *
 * The server's `detail` is raw rule text ("Only one source repository may be
 * primary per application."), so recognised conditions get their own message
 * and everything else falls back to the generic one with the message appended.
 */
function errorText(
  cause: unknown,
  t: PublishingTranslator,
  fallbackKey: "uploadFailed" | "uploadDriveLoadFailed",
): string {
  return t(fallbackKey, { message: messageOf(cause) })
}
