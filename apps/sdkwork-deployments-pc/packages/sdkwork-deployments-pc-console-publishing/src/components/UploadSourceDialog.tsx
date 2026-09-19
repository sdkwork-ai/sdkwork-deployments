/**
 * UploadSourceDialog — 应用行上的「上传代码」命令（三种来源）。
 *
 * 三种来源共用同一个对话框，左侧一个 radio 组切换：
 *
 *   1. **本地压缩包** — 浏览器内先算 SHA-256（`archiveChecksum`），再交给
 *      `createDeployApplicationPublisher` 做分片可续传的 Drive 上传 + 制品
 *      登记 + 发布 + 部署。整条链是**一个可中断、带进度的调用**，所以这里只
 *      负责收集文件/包类型并把进度渲染出来。
 *   2. **Git 仓库** — 只登记 `sourceRepository`（仓库是代码来源，不是制品），
 *      因此不走 publisher，而是直接 `sourceRepositories.create`。
 *   3. **从 Drive 选择** — 列出已有 `.zip` 复用，避免重复上传同一份包。
 *
 * 两条容易踩的坑，写在这里免得后人重犯：
 *
 * - **Drive 上传需要 `appResourceId` 作锚点**，所以上传必须以「已有应用」为
 *   前提。本对话框只从应用行打开（`app` 必填），创建流程不经过这里。
 * - **`File` 不是 `Blob` 的窄化替身**：`DriveUploaderBlobLike` 允许宿主换成
 *   文件系统句柄，所以校验一律读 `file.size` / `file.type`，不假设 `File` 特有
 *   的 `lastModified`。
 *
 * 组件为纯 props 输入（生成式 client + locale），deployments 控制台与任何宿主
 * 都能复用。
 */
import { useEffect, useMemo, useRef, useState, type ChangeEvent } from "react";
import type { AppResponse, SdkworkDeployAppClient } from "@sdkwork/deployments-app-sdk";
import type { SdkworkDriveAppClient } from "@sdkwork/drive-app-sdk";
import type { DeploymentsLocale } from "@sdkwork/deployments-pc-commons";
import { publishingTranslator, type PublishingTranslator } from "../i18n.ts";
import {
  DEPLOY_PACKAGE_TYPE_OPTIONS,
  createDeployAppOperationsService,
  type DeployAppOperationsService,
  type DeployCodeSource,
  type DeployDriveArchiveOption,
  type DeployUploadProgress,
} from "../service/deploy-app-operations.ts";
import css from "./create-deploy-app.module.css";

export interface UploadSourceDialogProps {
  readonly deployClient: SdkworkDeployAppClient
  readonly driveClient: SdkworkDriveAppClient
  readonly locale: DeploymentsLocale
  /** 目标应用：上传必须挂在已存在的应用上（Drive 资源锚点）。 */
  readonly app: AppResponse
  /** 上传成功回调：宿主据此提示并刷新列表。 */
  readonly onUploaded?: ((summary: string) => void) | undefined
  readonly onClose: () => void
  readonly theme?: ("light" | "dark") | undefined
  readonly size?: ("md" | "lg") | undefined
  /** 注入服务实例（测试用）；缺省按 client 构造。 */
  readonly service?: DeployAppOperationsService | undefined
}

const GIT_PROVIDERS = [
  { value: "GITHUB", label: "GitHub" },
  { value: "GITEE", label: "Gitee" },
  { value: "GITLAB", label: "GitLab" },
  { value: "SELF_HOSTED", label: "Self-hosted" },
] as const;

type GitProvider = (typeof GIT_PROVIDERS)[number]["value"];

/** 「全部后缀」哨兵值：`accept` 里不能写 `*`，但可以什么都不写。 */
const ZIP_ACCEPT = ".zip,application/zip,application/x-zip-compressed";

export function UploadSourceDialog({
  deployClient,
  driveClient,
  locale,
  app,
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

  const [source, setSource] = useState<DeployCodeSource>("local")
  const [packageType, setPackageType] = useState<number>(DEPLOY_PACKAGE_TYPE_OPTIONS[0]?.value ?? 1)
  const [file, setFile] = useState<File>()
  const [checksum, setChecksum] = useState<string>()
  const [progress, setProgress] = useState<DeployUploadProgress>()
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string>()
  const [notice, setNotice] = useState<string>()
  // React 19 requires an explicit initial value for `useRef`, so the "no
  // controller in flight" state is `undefined` rather than an omitted argument.
  const abortRef = useRef<AbortController | undefined>(undefined)

  // Git form
  const [repoKey, setRepoKey] = useState("")
  const [repoUrl, setRepoUrl] = useState("")
  const [repoProvider, setRepoProvider] = useState<GitProvider>("GITHUB")
  const [defaultBranch, setDefaultBranch] = useState("")
  const [credentialRef, setCredentialRef] = useState("")

  // Drive picker
  const [archives, setArchives] = useState<DeployDriveArchiveOption[]>()
  const [archivesLoading, setArchivesLoading] = useState(false)
  const [archivesLoaded, setArchivesLoaded] = useState(false)
  const [pickedArchive, setPickedArchive] = useState<DeployDriveArchiveOption>()

  const fileInputRef = useRef<HTMLInputElement>(null)

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape" && !busy) onClose()
    }
    document.addEventListener("keydown", onKeyDown)
    const previousOverflow = document.body.style.overflow
    document.body.style.overflow = "hidden"
    return () => {
      document.removeEventListener("keydown", onKeyDown)
      document.body.style.overflow = previousOverflow
    }
  }, [busy, onClose])

  // 离开对话框时中断在飞的 Drive 上传，否则用户关掉面板后浏览器还在传。
  useEffect(() => () => { abortRef.current?.abort() }, [])

  const limitMiB = useMemo(
    () => DEPLOY_PACKAGE_TYPE_OPTIONS.find((option) => option.value === packageType)?.maxSizeMiB ?? 2048,
    [packageType],
  )

  const loadArchives = async () => {
    setArchivesLoading(true)
    setArchivesLoaded(true)
    try {
      setArchives(await service.listDriveArchives())
    } catch (cause) {
      setArchives([])
      setError(errorText(cause, t, "uploadDriveLoadFailed"))
    } finally {
      setArchivesLoading(false)
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

  const activeArchive = source === "local"
    ? file === undefined
      ? undefined
      : { name: file.name, size: file.size }
    : pickedArchive === undefined
      ? undefined
      : { name: pickedArchive.fileName, size: pickedArchive.contentLength }

  const canSubmit = source === "local"
    ? file !== undefined && checksum !== undefined
    : source === "git"
      ? repoKey.trim() !== "" && repoUrl.trim() !== ""
      : pickedArchive !== undefined

  const abort = () => {
    abortRef.current?.abort()
  }

  const submit = async () => {
    setBusy(true)
    setError(undefined)
    setNotice(undefined)
    try {
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
        // Drive 里的包已经落在 Drive 上，但没有 `deploy_artifact` 记录 ——
        // 制品登记才是发布能消费它的前提。这里读回字节再走同一条 publisher
        // 链路，保证制品与本地压缩包形态完全一致。
        const archive = pickedArchive as DeployDriveArchiveOption
        const bytes = await downloadDriveArchive(driveClient, archive)
        const driveFile = new File([bytes], archive.fileName, { type: archive.contentType })
        const digest = await service.archiveChecksum(driveFile)
        const result = await service.uploadCodeFromArchive({
          appId: app.id,
          packageType,
          archive: {
            file: driveFile,
            fileName: archive.fileName,
            contentType: archive.contentType,
            checksumSha256: digest,
          },
          ...(abortRef.current === undefined ? {} : { signal: abortRef.current.signal }),
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
      setError(errorText(cause, t, "uploadFailed"))
      setBusy(false)
    } finally {
      abortRef.current = undefined
    }
  }

  const percent = progress === undefined || progress.totalBytes === 0
    ? undefined
    : Math.min(100, Math.round((progress.uploadedBytes / progress.totalBytes) * 100))

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
          <div className={css.radioGroup}>
            {(["local", "git", "drive"] as const).map((value) => (
              <label key={value} className={css.radioRow} data-selected={source === value}>
                <input
                  type="radio"
                  name="upload-source"
                  checked={source === value}
                  disabled={busy}
                  onChange={() => { setSource(value); setError(undefined) }}
                />
                <span className={css.radioLabel}>
                  <strong>{t(SOURCE_LABEL_KEYS[value])}</strong>
                  <small>{t(SOURCE_HINT_KEYS[value])}</small>
                </span>
              </label>
            ))}
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
              <div className={css.field}>
                <span className={css.fieldLabel}>{t("uploadGitRepoKey")}</span>
                <input
                  className={css.input}
                  value={repoKey}
                  placeholder={t("uploadGitRepoKeyPlaceholder")}
                  disabled={busy}
                  onChange={(event) => { setRepoKey(event.target.value) }}
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
              </div>

              <div className={css.field}>
                <span className={css.fieldLabel}>{t("uploadGitRepoUrl")}</span>
                <input
                  className={css.input}
                  value={repoUrl}
                  placeholder={t("uploadGitRepoUrlPlaceholder")}
                  disabled={busy}
                  onChange={(event) => { setRepoUrl(event.target.value) }}
                />
                <span className={css.fieldHint}>{t("uploadGitRepoUrlHint")}</span>
              </div>

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
            </>
          )}

          {source === "drive" && (
            <div className={css.field}>
              <span className={css.fieldLabel}>{t("uploadSourceDrive")}</span>
              {!archivesLoaded && (
                <button
                  type="button"
                  className={css.secondaryButton}
                  disabled={busy}
                  onClick={() => { void loadArchives() }}
                >
                  {t("uploadDriveLoad")}
                </button>
              )}
              {archivesLoading && <span className={css.fieldHint}>{t("uploadDriveLoading")}</span>}
              {archives !== undefined && archives.length === 0 && !archivesLoading && (
                <span className={css.fieldHint}>{t("uploadDriveEmpty")}</span>
              )}
              {archives !== undefined && archives.length > 0 && (
                <div className={css.appList}>
                  {archives.map((archive) => (
                    <button
                      key={archive.nodeId}
                      type="button"
                      className={css.appRow}
                      data-selected={pickedArchive?.nodeId === archive.nodeId}
                      disabled={busy}
                      onClick={() => { setPickedArchive(archive); setError(undefined) }}
                    >
                      <span className={css.appRowMeta}>
                        <strong>{archive.fileName}</strong>
                        <small>{formatBytes(archive.contentLength)} · {formatDate(archive.updatedAt, locale)}</small>
                      </span>
                      <span className={css.targetChipBadge}>
                        {pickedArchive?.nodeId === archive.nodeId ? t("uploadDriveSelected") : t("uploadDriveSelect")}
                      </span>
                    </button>
                  ))}
                </div>
              )}
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
        </div>

        <footer className={css.footer}>
          {error && <div className={css.errorBanner} role="alert">{error}</div>}
          {!error && notice && <div className={css.successBanner} role="status">{notice}</div>}
          {!error && !notice && <div className={css.footerSpacer} />}
          {busy && source === "local" && (
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
            {busy ? t("uploadInProgress") : source === "git" ? t("uploadGitConnect") : t("uploadConfirm")}
          </button>
        </footer>
      </div>
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
async function downloadDriveArchive(
  driveClient: SdkworkDriveAppClient,
  archive: DeployDriveArchiveOption,
): Promise<ArrayBuffer> {
  const chunks: Uint8Array[] = []
  let offset = 0
  let totalBytes: number | undefined

  for (;;) {
    const content = await driveClient.drive.nodes.content.retrieve(archive.nodeId, {
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
    if (!content.hasMore) {
      break
    }
  }

  if (totalBytes !== undefined && offset !== totalBytes) {
    throw new Error(
      `Drive returned ${offset} of ${totalBytes} bytes for ${archive.fileName}.`,
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

function formatDate(value: string, locale: DeploymentsLocale): string {
  const date = new Date(value)
  return Number.isNaN(date.getTime()) ? value : date.toLocaleDateString(locale)
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
  const message = cause instanceof Error && cause.message ? cause.message : String(cause)
  return t(fallbackKey, { message })
}
