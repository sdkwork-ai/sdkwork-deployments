/**
 * Row operations of the applications ledger.
 *
 * These mirror the four operations the Web Server console's own application
 * ledger used to expose per row (`update` / `update-source` / `publish` /
 * `delete`, see the retired `isApplicationRowAction`). Applications moved to the
 * deploy plane, so the same semantics are re-expressed over `deploy_app`:
 *
 * - `edit`     -> deploy_app metadata (name / description)
 * - `code`     -> deploy_app_source_repository
 * - `publish`  -> deploy_app_release, cut from an already-registered package
 * - `delete`   -> deliberately has no dialog: the app-api contract defines no
 *                 `apps.delete`, so the ledger renders that slot disabled with
 *                 the reason instead of a button that cannot act.
 *
 * Every call goes through the injected `DeployAppPublishingService`, never the
 * generated clients, so the SDK surface stays owned by one place. Markup reuses
 * the class vocabulary of the delivery ledger's dialogs (`dialog-backdrop`,
 * `dialog`, `form-grid`, `dialog-footer`, `*-button`), which both hosts already
 * style — the deployments console from `styles.css`, the Web Server console from
 * the `.deploy-surface` mirror — so these components ship no stylesheet of their
 * own and need no second registration.
 */
import type { AppResponse } from "@sdkwork/deployments-app-sdk";
import type { DeploymentsLocale } from "@sdkwork/deployments-pc-commons";
import { useEffect, useMemo, useState, type ReactNode } from "react";
import { publishingTranslator, type PublishingTranslator } from "../i18n.ts";
import { isValidSemver, type DeployAppPublishingService } from "../service/deploy-app-publishing.ts";

/** Repo hosts the app-api accepts; the picker offers exactly the contract's set. */
const REPO_PROVIDERS = ["GITHUB", "GITEE", "GITLAB", "SELF_HOSTED"] as const;
const CLONE_MODES = ["FULL", "SHALLOW"] as const;

function messageOf(cause: unknown): string {
  return cause instanceof Error ? cause.message : String(cause);
}

/**
 * Modal shell.
 *
 * A local shell rather than a shared one on purpose: the two delivery-ledger
 * dialogs are package-private too, and lifting a modal into a shared package for
 * three call sites would couple the publishing package to the delivery one.
 */
function Modal({
  children,
  close,
  closeLabel,
  title,
}: {
  children: ReactNode;
  close(): void;
  closeLabel: string;
  title: string;
}) {
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") close();
    };
    document.addEventListener("keydown", onKeyDown);
    return () => document.removeEventListener("keydown", onKeyDown);
  }, [close]);
  return (
    <div
      className="dialog-backdrop"
      role="presentation"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) close();
      }}
    >
      <div className="dialog delivery-dialog" role="dialog" aria-modal="true" aria-labelledby="app-operations-title">
        <header>
          <h2 id="app-operations-title">{title}</h2>
          <button className="icon-button" type="button" title={closeLabel} aria-label={closeLabel} onClick={close}>
            ×
          </button>
        </header>
        {children}
      </div>
    </div>
  );
}

/** Cancel / submit pair shared by the three dialogs. */
function DialogFooter({
  busy,
  close,
  disabled = false,
  submitLabel,
  t,
  onSubmit,
}: {
  busy: boolean;
  close(): void;
  disabled?: boolean;
  submitLabel: string;
  t: PublishingTranslator;
  onSubmit(): void;
}) {
  return (
    <footer className="dialog-footer">
      <button className="secondary-button" type="button" onClick={close}>
        {t("cancel")}
      </button>
      <button className="command-button" type="button" disabled={busy || disabled} onClick={onSubmit}>
        {busy ? t("saving") : submitLabel}
      </button>
    </footer>
  );
}

export interface AppEditDialogProps {
  readonly app: AppResponse;
  readonly locale: DeploymentsLocale;
  readonly service: DeployAppPublishingService;
  readonly onClose: () => void;
  readonly onSaved: () => void;
}

/** `update`: edit the application's own metadata. */
export function AppEditDialog({ app, locale, onClose, onSaved, service }: AppEditDialogProps) {
  const t = useMemo(() => publishingTranslator(locale), [locale]);
  const [name, setName] = useState(app.name);
  const [description, setDescription] = useState(app.description ?? "");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>();

  async function submit(): Promise<void> {
    setBusy(true);
    setError(undefined);
    try {
      await service.updateApp(app.id, { name, description });
      onSaved();
    } catch (cause) {
      setError(t("operationFailed", { message: messageOf(cause) }));
      setBusy(false);
    }
  }

  return (
    <Modal close={onClose} closeLabel={t("close")} title={t("editAppTitle")}>
      <div className="form-grid">
        <label>
          <span>{t("applicationName")}</span>
          <input
            autoFocus
            value={name}
            onChange={(event) => setName(event.target.value)}
            autoComplete="off"
          />
        </label>
        <label>
          <span>{t("description")}</span>
          <input
            value={description}
            onChange={(event) => setDescription(event.target.value)}
            autoComplete="off"
          />
        </label>
      </div>
      {error && <div className="error-banner" role="alert">{error}</div>}
      <DialogFooter
        busy={busy}
        close={onClose}
        disabled={name.trim().length === 0}
        submitLabel={t("save")}
        t={t}
        onSubmit={() => void submit()}
      />
    </Modal>
  );
}

export interface AppSourceDialogProps {
  readonly app: AppResponse;
  readonly locale: DeploymentsLocale;
  readonly service: DeployAppPublishingService;
  readonly onClose: () => void;
  readonly onSaved: () => void;
}

/** `update-source`: attach the git repository the app builds from. */
export function AppSourceDialog({ app, locale, onClose, onSaved, service }: AppSourceDialogProps) {
  const t = useMemo(() => publishingTranslator(locale), [locale]);
  const [repoKey, setRepoKey] = useState(app.slug);
  const [repoProvider, setRepoProvider] = useState<(typeof REPO_PROVIDERS)[number]>("GITHUB");
  const [repoUrl, setRepoUrl] = useState("");
  const [defaultBranch, setDefaultBranch] = useState("");
  const [cloneMode, setCloneMode] = useState<(typeof CLONE_MODES)[number]>("FULL");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>();

  const ready = repoKey.trim().length > 0 && repoUrl.trim().length > 0;

  async function submit(): Promise<void> {
    setBusy(true);
    setError(undefined);
    try {
      await service.createSourceRepository(app.id, { cloneMode, defaultBranch, repoKey, repoProvider, repoUrl });
      onSaved();
    } catch (cause) {
      setError(t("operationFailed", { message: messageOf(cause) }));
      setBusy(false);
    }
  }

  return (
    <Modal close={onClose} closeLabel={t("close")} title={t("updateSourceTitle")}>
      <div className="form-grid">
        <label>
          <span>{t("sourceRepoKey")}</span>
          <input value={repoKey} onChange={(event) => setRepoKey(event.target.value)} autoComplete="off" />
        </label>
        <label>
          <span>{t("sourceRepoProvider")}</span>
          <select
            value={repoProvider}
            onChange={(event) => setRepoProvider(event.target.value as (typeof REPO_PROVIDERS)[number])}
          >
            {REPO_PROVIDERS.map((provider) => <option key={provider} value={provider}>{provider}</option>)}
          </select>
        </label>
        <label>
          <span>{t("sourceRepoUrl")}</span>
          <input
            autoFocus
            placeholder="https://github.com/owner/repository.git"
            value={repoUrl}
            onChange={(event) => setRepoUrl(event.target.value)}
            autoComplete="off"
          />
        </label>
        <label>
          <span>{t("sourceDefaultBranch")}</span>
          <input value={defaultBranch} onChange={(event) => setDefaultBranch(event.target.value)} autoComplete="off" />
        </label>
        <label>
          <span>{t("sourceCloneMode")}</span>
          <select value={cloneMode} onChange={(event) => setCloneMode(event.target.value as (typeof CLONE_MODES)[number])}>
            {CLONE_MODES.map((mode) => <option key={mode} value={mode}>{mode}</option>)}
          </select>
        </label>
      </div>
      {error && <div className="error-banner" role="alert">{error}</div>}
      <DialogFooter busy={busy} close={onClose} disabled={!ready} submitLabel={t("save")} t={t} onSubmit={() => void submit()} />
    </Modal>
  );
}

export interface AppPublishDialogProps {
  readonly app: AppResponse;
  readonly locale: DeploymentsLocale;
  readonly service: DeployAppPublishingService;
  readonly onClose: () => void;
  readonly onSaved: () => void;
}

/**
 * `publish`: cut a release.
 *
 * A release is `(platform target, package, semantic version)`, so the dialog
 * loads the app's registered packages and platform targets rather than asking
 * the operator to type identifiers. Both lists can legitimately be empty — a
 * brand-new app has neither — and the empty state says which step is missing
 * instead of offering a submit that would fail server-side.
 */
export function AppPublishDialog({ app, locale, onClose, onSaved, service }: AppPublishDialogProps) {
  const t = useMemo(() => publishingTranslator(locale), [locale]);
  const [targets, setTargets] = useState<readonly { id: string; label: string }[]>();
  const [packages, setPackages] = useState<readonly { id: string; label: string; version: string }[]>();
  const [platformTargetId, setPlatformTargetId] = useState("");
  const [packageId, setPackageId] = useState("");
  const [version, setVersion] = useState("");
  const [notes, setNotes] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>();

  useEffect(() => {
    let active = true;
    void Promise.all([service.listPlatformTargets(app.id), service.listPackages(app.id)])
      .then(([targetRows, packageRows]) => {
        if (!active) return;
        setTargets(targetRows.map((row) => ({
          id: row.id,
          label: `${row.targetKey} · ${row.platform}`,
        })));
        setPackages(packageRows.map((row) => ({
          id: row.id,
          label: `${row.packageFormat} · ${row.packageStatus}`,
          version: row.semanticVersion,
        })));
      })
      .catch((cause: unknown) => {
        if (active) setError(t("operationFailed", { message: messageOf(cause) }));
      });
    return () => { active = false; };
  }, [app.id, service, t]);

  const options = targets !== undefined && packages !== undefined;
  const versionValid = isValidSemver(version);
  const ready = platformTargetId.length > 0 && packageId.length > 0 && versionValid;

  async function submit(): Promise<void> {
    setBusy(true);
    setError(undefined);
    try {
      await service.publishAppRelease(app.id, { packageId, platformTargetId, releaseNotes: notes, semanticVersion: version });
      onSaved();
    } catch (cause) {
      setError(t("operationFailed", { message: messageOf(cause) }));
      setBusy(false);
    }
  }

  return (
    <Modal close={onClose} closeLabel={t("close")} title={t("publishReleaseTitle")}>
      {!options ? (
        <p className="form-hint">{t("optionsLoading")}</p>
      ) : targets.length === 0 ? (
        <p className="form-hint">{t("releaseNoTargets")}</p>
      ) : packages.length === 0 ? (
        <p className="form-hint">{t("releaseNoPackages")}</p>
      ) : (
        <div className="form-grid">
          <label>
            <span>{t("releasePlatformTarget")}</span>
            <select value={platformTargetId} onChange={(event) => setPlatformTargetId(event.target.value)}>
              <option value="">-</option>
              {targets.map((target) => <option key={target.id} value={target.id}>{target.label}</option>)}
            </select>
          </label>
          <label>
            <span>{t("releasePackage")}</span>
            <select
              value={packageId}
              onChange={(event) => {
                setPackageId(event.target.value);
                // A package already carries the version it was built as, so it is
                // the operator's default; typing over it still wins.
                const picked = packages.find((candidate) => candidate.id === event.target.value);
                if (picked !== undefined && version.trim().length === 0) setVersion(picked.version);
              }}
            >
              <option value="">-</option>
              {packages.map((pkg) => <option key={pkg.id} value={pkg.id}>{`${pkg.label} · ${pkg.version}`}</option>)}
            </select>
          </label>
          <label>
            <span>{t("version")}</span>
            <input
              value={version}
              placeholder={t("versionPlaceholder")}
              aria-invalid={version.trim() !== "" && !versionValid}
              onChange={(event) => setVersion(event.target.value)}
              autoComplete="off"
            />
            {version.trim() !== "" && !versionValid && (
              <span className="form-error" role="alert">{t("versionError")}</span>
            )}
          </label>
          <label>
            <span>{t("releaseNote")}</span>
            <input value={notes} onChange={(event) => setNotes(event.target.value)} autoComplete="off" />
          </label>
        </div>
      )}
      {error && <div className="error-banner" role="alert">{error}</div>}
      <DialogFooter busy={busy} close={onClose} disabled={!ready} submitLabel={t("publishRelease")} t={t} onSubmit={() => void submit()} />
    </Modal>
  );
}
