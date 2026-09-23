import type { DeploymentsLocale, DeploymentsResourcePageProps } from "@sdkwork/deployments-pc-commons";
import {
  type CertificateRenewalResponse,
  type CertificateResponse,
  type CloudAccountDnsProvider,
  type CloudAccountRegistrationResponse,
  type CloudAccountResponse,
  type DomainHostnameClaimResponse,
  type DomainHostnameResponse,
  type DomainVerifyResponse,
  type DomainZoneResponse,
  type PageInfo,
  useDeploymentsDeliveryService,
} from "@sdkwork/deployments-pc-console-core";
import {
  ArrowLeft,
  BadgeCheck,
  ChevronDown,
  CirclePause,
  CirclePlay,
  Clipboard,
  ClipboardList,
  FileKey2,
  Globe2,
  History,
  Pencil,
  Plus,
  RefreshCw,
  RotateCw,
  Search,
  ShieldCheck,
  Trash2,
  X,
} from "lucide-react";
import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type FormEvent,
  type KeyboardEvent,
  type ReactNode,
} from "react";
import { Link, Navigate, Route, Routes, useParams, useSearchParams } from "react-router-dom";

import { DataTable, type DataTableColumn, type DataTablePaginationProps } from "@sdkwork/ui-pc-react";

import { deliveryText, type DeliveryMessageKey } from "./i18n.ts";
import { relativeRecordName } from "./dns-record-name.ts";
import { type RootDomainIssue, isRootDomainApex, validateRootDomain } from "./root-domain.ts";

type Translator = (key: DeliveryMessageKey, values?: Record<string, string | number>) => string;
type ZoneDialog =
  | { kind: "create" }
  | { kind: "edit"; zone: DomainZoneResponse }
  | { kind: "status"; zone: DomainZoneResponse }
  | { kind: "delete"; zone: DomainZoneResponse };

export function DomainManagementPage({ locale }: DeploymentsResourcePageProps) {
  return <Routes>
    <Route index element={<DomainZoneList locale={locale} />} />
    <Route path=":zoneId" element={<DomainHostnameList locale={locale} />} />
    <Route path="*" element={<Navigate to="/console/domains" replace />} />
  </Routes>;
}

function DomainZoneList({ locale }: { locale: DeploymentsLocale }) {
  const service = useDeploymentsDeliveryService();
  const t = translator(locale);
  const [zones, setZones] = useState<DomainZoneResponse[]>([]);
  const [pageInfo, setPageInfo] = useState<PageInfo>({ mode: "offset", page: 1, pageSize: 20, hasMore: false });
  const [page, setPage] = useState(1);
  const [pageSize, setPageSize] = useState(20);
  const [searchDraft, setSearchDraft] = useState("");
  const [keyword, setKeyword] = useState("");
  const [status, setStatus] = useState<"ALL" | "ACTIVE" | "PAUSED">("ALL");
  const [refreshVersion, setRefreshVersion] = useState(0);
  const [dialog, setDialog] = useState<ZoneDialog>();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>();

  useEffect(() => {
    let active = true;
    setBusy(true);
    setError(undefined);
    // This is the root-domain list, so it asks for root domains only. The
    // inventory also holds tenant-level `app.<suffix>` zones, but those are
    // subdomains the deployment provisions for app publishing — they are not
    // root domains and must never be listed here as if the operator had
    // registered them. `scope=USER` means `user_id IS NOT NULL`, which is
    // exactly "an operator-defined root domain". Their subdomains are reached
    // by opening the root domain, not by showing up as siblings of it.
    void service.listDomainZones({
      page,
      pageSize,
      keyword: keyword || undefined,
      status: status === "ALL" ? undefined : status,
      scope: "USER",
    }).then((result) => {
      if (!active) return;
      // This table is the root-domain list, so every row it shows has to be a
      // root domain. The platform provisions one `app.<suffix>` zone per suffix
      // it serves for the whole tenant, and those land in the same listing as
      // the operator's own root domains - but an `app.<suffix>` apex is a
      // subdomain, not a root domain, and the hostnames under it belong to the
      // zone it names rather than to any root domain on this page.
      //
      // The listing also sends `scope: "USER"` so the service leaves them out
      // server-side, but the shape filter is applied here as well: a gateway
      // that predates the parameter drops it without an error and answers with
      // the whole inventory, and the list's own invariant is "root domains
      // only" — so it holds that invariant rather than trusting a response to
      // have honoured a request. A shape filter also survives a response that
      // carries no ownership field at all, where a `scope` comparison would
      // silently hide every row.
      const rootDomains = result.items.filter((zone) => isRootDomainApex(zone.apexHostname));
      setZones(rootDomains);
      // Keep the count honest when rows were dropped that the service still
      // counted. "共 N 条" sitting above a visibly shorter list is the one
      // inconsistency an operator reads as a bug; once the service filters too
      // the two agree and this assignment is a no-op.
      setPageInfo(
        rootDomains.length === result.items.length
          ? result.pageInfo
          : { ...result.pageInfo, totalItems: String(rootDomains.length) },
      );
    }).catch((cause) => {
      if (active) setError(errorText(cause));
    }).finally(() => {
      if (active) setBusy(false);
    });
    return () => { active = false; };
  }, [keyword, page, pageSize, refreshVersion, service, status]);

  const reload = () => setRefreshVersion((value) => value + 1);
  const closeAndReload = () => { setDialog(undefined); reload(); };

  /**
   * 根域名列：整格是一个 Link（原实现如此）—— 打开根域名是进入其子域名的
   * 唯一入口，所以单元格本身就是导航，不是纯文本。
   */
  const zoneColumns = useMemo<DataTableColumn<DomainZoneResponse>[]>(() => [
    {
      id: "apexHostname",
      header: t("rootDomain"),
      cell: (zone) => (
        // Opening the root domain is how its subdomains are reached, so the
        // apex cell and the 子域名 action both lead to the same place.
        <Link className="primary-cell-link" to={zone.id}>
          <Globe2 size={17} />
          <span>
            <strong>{zone.apexHostname}</strong>
            <small>{zone.displayName || zone.dnsProvider || "-"}</small>
          </span>
        </Link>
      ),
      width: 260,
    },
    { id: "status", header: t("status"), cell: (zone) => <StatusBadge value={zone.status} t={t} />, width: 110 },
    {
      id: "hostnameCount",
      header: t("hostnames"),
      cell: (zone) => <><strong>{zone.hostnameCount}</strong><small className="cell-subtitle">{t("verifiedSummary", { verified: zone.verifiedHostnameCount, total: zone.hostnameCount })}</small></>,
      width: 160,
    },
    { id: "certificateCount", header: t("certificates"), cell: (zone) => zone.certificateCount, width: 110 },
    { id: "bindingCount", header: t("appBindings"), cell: (zone) => zone.bindingCount, width: 110 },
    { id: "updatedAt", header: t("updated"), cell: (zone) => formatDate(zone.updatedAt, locale), width: 180 },
  ], [locale, t]);

  return <section className="resource-page domain-page">
    <div className="resource-commandbar">
      <div className="resource-identity"><h1>{t("domainsTitle")}</h1></div>
      <div className="resource-query">
        <form className="search-box" onSubmit={(event) => { event.preventDefault(); setPage(1); setKeyword(searchDraft.trim()); }}>
          <Search size={16} /><input aria-label={t("search")} value={searchDraft} onChange={(event) => setSearchDraft(event.target.value)} placeholder={t("search")} />
        </form>
        <div className="segmented-control" aria-label={t("status")}>
          {(["ALL", "ACTIVE", "PAUSED"] as const).map((value) => <button key={value} type="button" aria-pressed={status === value} onClick={() => { setPage(1); setStatus(value); }}>{value === "ALL" ? t("all") : value === "ACTIVE" ? t("active") : t("paused")}</button>)}
        </div>
      </div>
      <div className="actions">
        <button className="icon-button" type="button" disabled={busy} title={t("refresh")} onClick={reload}><RefreshCw size={17} /></button>
        <button className="command-button" type="button" onClick={() => setDialog({ kind: "create" })}><Plus size={16} />{t("defineRoot")}</button>
      </div>
    </div>
    {error && <ErrorBanner message={error} t={t} />}
    <DataTable<DomainZoneResponse>
      columns={zoneColumns}
      density="compact"
      emptyState={<span><Globe2 size={24} />{t("noRootDomains")}</span>}
      getRowId={(zone) => zone.id}
      loading={busy && zones.length === 0}
      pagination={serverPagination(page, pageInfo, busy, setPage, setPageSize)}
      rowActions={(zone) => {
        // hostnameCount includes the apex hostname row every zone owns, so
        // only counts above 1 represent user-added subdomains that block
        // zone deletion.
        const deleteBlocked = Number(zone.hostnameCount) > 1 || Number(zone.certificateCount) > 0 || Number(zone.bindingCount) > 0;
        return <div className="row-actions">
          {/* Text labels rather than bare icons: entering the subdomain list and
              requesting a certificate are the two things an operator opens this
              table for, and neither is guessable from a glyph alone. The literal
              text stays inside the accessible name so the label still matches
              what is read out. */}
          <Link className="table-action table-action-text" to={zone.id} title={t("open")} aria-label={`${t("hostnames")} · ${zone.apexHostname}`}><Globe2 size={15} /><span>{t("hostnames")}</span></Link>
          <Link className="table-action table-action-text" to={`/console/certificates?zoneId=${encodeURIComponent(zone.id)}&apex=${encodeURIComponent(zone.apexHostname)}`} title={t("requestCertificate")} aria-label={`${t("certificates")} · ${zone.apexHostname}`}><FileKey2 size={15} /><span>{t("certificates")}</span></Link>
          <button className="table-action" type="button" title={t("edit")} aria-label={`${t("edit")} ${zone.apexHostname}`} onClick={() => setDialog({ kind: "edit", zone })}><Pencil size={16} /></button>
          <button className="table-action" type="button" title={zone.status === "ACTIVE" ? t("pause") : t("resume")} aria-label={`${zone.status === "ACTIVE" ? t("pause") : t("resume")} ${zone.apexHostname}`} onClick={() => setDialog({ kind: "status", zone })}>{zone.status === "ACTIVE" ? <CirclePause size={16} /> : <CirclePlay size={16} />}</button>
          <button className="table-action danger-action" type="button" disabled={deleteBlocked} title={deleteBlocked ? t("deleteBlocked") : t("delete")} aria-label={`${t("delete")} ${zone.apexHostname}`} onClick={() => setDialog({ kind: "delete", zone })}><Trash2 size={16} /></button>
        </div>;
      }}
      rowActionsLabel={t("operations")}
      rows={zones}
      stickyHeader
    />
    {dialog?.kind === "create" && <ZoneFormDialog t={t} close={() => setDialog(undefined)} submit={async (body) => { await service.createDomainZone(toDomainZoneRequestBody(body)); closeAndReload(); }} />}
    {dialog?.kind === "edit" && <ZoneFormDialog t={t} zone={dialog.zone} close={() => setDialog(undefined)} submit={async (body) => { await service.updateDomainZone(dialog.zone.id, toDomainZoneRequestBody(body)); closeAndReload(); }} />}
    {dialog?.kind === "status" && <ConfirmDialog
      title={dialog.zone.status === "ACTIVE" ? t("pauseZoneTitle") : t("resumeZoneTitle")}
      message={dialog.zone.status === "ACTIVE" ? t("pauseZoneConfirm") : t("resumeZoneConfirm")}
      dangerous={dialog.zone.status === "ACTIVE"}
      t={t}
      close={() => setDialog(undefined)}
      submit={async () => { await service.updateDomainZone(dialog.zone.id, { status: dialog.zone.status === "ACTIVE" ? "PAUSED" : "ACTIVE" }); closeAndReload(); }}
    />}
    {dialog?.kind === "delete" && <ConfirmDialog title={t("deleteZoneTitle")} message={t("deleteZoneConfirm")} dangerous t={t} close={() => setDialog(undefined)} submit={async () => { await service.deleteDomainZone(dialog.zone.id); closeAndReload(); }} />}
  </section>;
}

/**
 * Zone form payload.
 *
 * Every optional member except `providerAccountId` is a plain "left blank"
 * state that maps to an omitted key. `providerAccountId` carries a third
 * meaning — the empty string clears an existing pin — and is documented on the
 * member itself.
 */
export interface DomainZoneFormBody {
  apexHostname: string;
  displayName?: string | undefined;
  dnsProvider?: string | undefined;
  providerZoneRef?: string | undefined;
  /**
   * The cloud account every hostname in this root domain presents with.
   *
   * Three states, and all three reach the wire:
   *
   * * absent — untouched, so an update leaves any existing pin alone;
   * * `""` — the operator cleared the pin, which the contract reads as "remove it";
   * * a value — pin that account.
   *
   * The empty string is therefore meaningful rather than a blank field, which is
   * why it is passed through instead of being folded into `undefined` the way the
   * other optional members are.
   */
  providerAccountId?: string | undefined;
}

/**
 * Map the form payload onto the generated create/update request.
 *
 * Generated request types declare their optionals as `field?: string`, so an
 * explicit `undefined` is rejected under `exactOptionalPropertyTypes`; hand
 * editing generated output is forbidden, so blank fields are omitted here. The
 * wire treats an absent key and an explicit `undefined` identically.
 */
export function toDomainZoneRequestBody(body: DomainZoneFormBody) {
  return {
    apexHostname: body.apexHostname,
    ...(body.displayName === undefined ? {} : { displayName: body.displayName }),
    ...(body.dnsProvider === undefined ? {} : { dnsProvider: body.dnsProvider }),
    ...(body.providerZoneRef === undefined ? {} : { providerZoneRef: body.providerZoneRef }),
    ...(body.providerAccountId === undefined ? {} : { providerAccountId: body.providerAccountId }),
  };
}

/**
 * Canonical form of a declared provider: lowercase, spaces and runs of
 * underscores collapsed to a single underscore.
 *
 * One spelling function for every reader, so `Aliyun DNS`, `aliyun_dns` and
 * `aliyun  dns` cannot be read one way by the family lookup and another way by the
 * `manual` check.
 */
function normalizeProvider(declared: string | undefined): string {
  return (declared ?? "").trim().toLowerCase().replaceAll(" ", "_").replace(/_+/g, "_");
}

/**
 * The DNS family a declared provider drives, or `undefined` when it drives none.
 *
 * `deploy_dns_zone.dns_provider` is free text: the contract keeps it that way
 * because `manual` is a legitimate declaration meaning the operator publishes
 * records by hand, and because the column predates the account center. The
 * picker still needs to know which family to filter by, so the spellings the
 * server's own `family_for_vendor_code` accepts are mirrored here. Anything else
 * — `manual`, a typo, a provider this build cannot drive — yields no filter
 * rather than a guess, and the picker then offers every account.
 */
export function dnsFamilyFromDeclared(declared: string | undefined): CloudAccountDnsProvider | undefined {
  const normalized = normalizeProvider(declared);
  if (!normalized) return undefined;
  switch (normalized.replace(/_dns$/, "")) {
    case "aliyun":
    case "ali":
    case "alidns":
      return "ALIYUN_DNS";
    case "tencent":
    case "dnspod":
    case "qcloud":
      return "DNSPOD";
    case "cloudflare":
    case "cf":
      return "CLOUDFLARE";
    default:
      return undefined;
  }
}

/**
 * True when a declared provider means the records are published by hand.
 *
 * `manual` is not a missing answer — it is the answer "no automation", and it is
 * the reason the column stays free text. The account field has to tell it apart
 * from a provider nobody has decided yet: one needs to hear that there is nothing
 * to choose here, the other needs to hear that a choice will not be filtered.
 */
export function providerIsManual(declared: string | undefined): boolean {
  return normalizeProvider(declared) === "manual";
}

/** Families the account center can be asked for, in the order the console lists them. */
const DNS_FAMILIES: readonly CloudAccountDnsProvider[] = ["ALIYUN_DNS", "DNSPOD", "CLOUDFLARE"];

/**
 * Spellings offered for `deploy_dns_zone.dns_provider`.
 *
 * The column is free text on purpose — `manual` is a real declaration (records are
 * published by hand, so no account is involved at all) and the column predates the
 * account center. These are suggestions, not a closed list, so an unrecognised
 * value stays expressible; but the common ones no longer have to be guessed, and
 * guessing is exactly what breaks the account filter: a provider this build cannot
 * read yields no family, and the account picker then offers every account as if the
 * provider had never been named.
 */
const DNS_PROVIDER_SUGGESTIONS: readonly string[] = ["Aliyun DNS", "DNSPod", "Cloudflare", "manual"];

/**
 * Display name for a DNS family.
 *
 * Accepts a plain string, not the family union, because the account response
 * carries `dnsProvider` as a string: it is absent for a vendor this build cannot
 * drive, and that case has to render as something rather than crash.
 */
function dnsFamilyLabel(family: string | undefined): string {
  switch (family) {
    case "ALIYUN_DNS": return "Aliyun DNS";
    case "DNSPOD": return "DNSPod";
    case "CLOUDFLARE": return "Cloudflare";
    case undefined: return "-";
    default: return family;
  }
}

/** Which level of the tenant an account is visible at. */
function accountScopeLabel(account: CloudAccountResponse, t: Translator): string {
  if (account.scopeType === "platform") return t("cloudAccountScopePlatform");
  if (account.scopeType === "user") return t("cloudAccountScopeUser");
  return t("cloudAccountScopeTenant");
}

/**
 * Binds a root domain or certificate to one cloud account.
 *
 * The value is an account id, or `undefined` for "automatic". Automatic is the
 * default because the server's own chain is usually right: a tenant that
 * registered an Aliyun key once should not have to pin it on every root domain,
 * and pinning freezes a choice that would otherwise follow the account center.
 */
function CloudAccountField({ declaredProvider, dnsFamily, familyNote, onChange, recordsPublishedByHand = false, t, value }: {
  /**
   * The provider exactly as the row declares it, passed only when this build
   * cannot read that spelling.
   *
   * `dnsFamily` cannot carry this: it is `undefined` both for a provider nobody
   * has named and for one named in a spelling this build does not recognise, and
   * those two need opposite sentences. One asks the operator to decide; the other
   * has to admit their decision was not understood, because calling a decided
   * provider "not decided yet" sends them back to a form to set what is already
   * set while the list below stays quietly unfiltered.
   */
  declaredProvider?: string | undefined;
  dnsFamily: CloudAccountDnsProvider | undefined;
  /**
   * Translated sentence naming where `dnsFamily` came from, when it was decided
   * somewhere other than this field (a root domain's declaration, or the provider
   * typed into the same form). Shown only while nothing is pinned, because once an
   * account is chosen the question "why is this list filtered" is answered.
   */
  familyNote?: string | undefined;
  onChange(next: string | undefined): void;
  /**
   * The provider is declared `manual`, so no account takes part at all: the
   * operator publishes the TXT record themselves. Distinct from an undecided
   * provider, which is what makes it worth a prop — one state has nothing to
   * choose, the other has a choice that will not be filtered.
   */
  recordsPublishedByHand?: boolean;
  t: Translator;
  value: string | undefined;
}) {
  const service = useDeploymentsDeliveryService();
  const [accounts, setAccounts] = useState<readonly CloudAccountResponse[]>([]);
  const [loaded, setLoaded] = useState(false);
  const [loadError, setLoadError] = useState<string>();
  // Holds the row returned by a registration, which is by definition not in
  // `accounts` yet: the list was read before it existed.
  const [adopted, setAdopted] = useState<CloudAccountResponse>();
  const [pickerOpen, setPickerOpen] = useState(false);
  const [registerOpen, setRegisterOpen] = useState(false);
  const [reused, setReused] = useState(false);

  // One unfiltered read answers both questions this field has: which row a pin
  // points at, and whether exactly one candidate exists worth offering as a
  // shortcut. Resolving the pin through a family-filtered list would report a
  // provider mismatch as "no account exists", which says the wrong thing — the
  // account does exist, it just cannot answer for this domain.
  useEffect(() => {
    let active = true;
    void service.listCloudAccounts({ page: 1, pageSize: 200 })
      .then((result) => { if (active) { setAccounts(result.items); setLoaded(true); } })
      .catch((cause) => { if (active) { setLoadError(errorText(cause)); setLoaded(true); } });
    return () => { active = false; };
  }, [service]);

  // Resolution is derived rather than stored, so it cannot go stale against the
  // list, and the read does not have to repeat every time the pin changes.
  const chosen = value === undefined
    ? undefined
    : adopted?.id === value
      ? adopted
      : accounts.find((account) => account.id === value);
  // A pin that no longer resolves is reported rather than silently rendered as
  // "automatic": the two states publish the record through different credentials,
  // and the operator is the one who has to fix it — which they cannot do if the
  // field pretends nothing is set. An unanswered or failed read proves nothing, so
  // the error is raised only once a successful list came back without the id.
  const pinError = value !== undefined && loaded && loadError === undefined && chosen === undefined
    ? t("cloudAccountMissing")
    : undefined;
  // A pin that resolves is still the wrong answer when it belongs to another
  // provider: the server would publish this domain's records with a credential
  // that cannot reach the zone, and nothing says so until the first order fails.
  // The picker can produce this on purpose — its family select offers "all" —
  // and an edit can inherit it from a root domain whose provider has since
  // changed. Unresolvable pins are a different message and take precedence.
  const chosenFamilyMismatch = chosen === undefined || dnsFamily === undefined
    ? undefined
    : chosen.dnsProvider === undefined
      ? t("cloudAccountFamilyNoDns")
      : chosen.dnsProvider === dnsFamily
        ? undefined
        : t("cloudAccountFamilyMismatch", {
          accountFamily: dnsFamilyLabel(chosen.dnsProvider),
          family: dnsFamilyLabel(dnsFamily),
        });
  // A failed read is the more useful thing to show when both happened: the pin
  // cannot be judged at all until the list answers.
  const shownError = loadError ?? pinError ?? chosenFamilyMismatch;

  const candidates = dnsFamily === undefined
    ? []
    : accounts.filter((account) => account.dnsProvider === dnsFamily);
  // The shortcut is offered only when the single candidate is one the server would
  // accept, so it can never propose a bind that fails. The picker deliberately
  // stays looser: it lists credential-less rows too, labelled, because learning
  // that an account exists but carries no secret is itself useful.
  const sole = candidates.length === 1 ? candidates[0] : undefined;
  const suggestion = value === undefined && sole !== undefined
    && sole.credentialConfigured && sole.status === "active"
    ? sole
    : undefined;

  function adopt(account: CloudAccountResponse) {
    setAdopted(account);
    onChange(account.id);
    setPickerOpen(false);
  }

  return <fieldset className="form-fieldset form-field-wide">
    {/* The resolution order — this certificate's account, then its root
        domain's, then the one the account center picks for the declared
        provider, then the deployment-wide credential — is a rule the operator
        consults the moment the automatic answer surprises them, not a sentence
        they re-read on every open. It rides on the caption instead of taking up
        to three lines beside the buttons, which is what made this the tallest
        block in the dialog. Withheld for hand-published records, where the
        server is never asked to resolve anything. */}
    <legend title={chosen === undefined && !recordsPublishedByHand ? t("cloudAccountAutoHint") : undefined}>{t("cloudAccount")}</legend>
    {/* Two rows: the answer, then the ways to change it.
        They used to share one row, which left the answer a quarter of the track —
        the account's own name was the narrowest thing in the block while three
        equally-weighted buttons took the rest, and "use automatic" read as a peer
        of "choose" and "register" when it is a revision of the answer rather than
        a third way to produce one. The answer therefore owns a full row, and "use
        automatic" drops to a quiet control on that row. The row is only drawn as a
        box when something is pinned, so the box means "a value is set" instead of
        decorating the automatic state, which is the state the operator sees most
        often and the one with the longest caption. */}
    <div className="cloud-account-selection" data-state={chosen === undefined ? "auto" : "pinned"}>
      <span className="cloud-account-mark" aria-hidden="true">{chosen === undefined ? <RotateCw size={15} /> : <BadgeCheck size={15} />}</span>
      <div className="cloud-account-summary">
        <strong>{chosen ? chosen.displayName : t("cloudAccountAuto")}</strong>
        {/* The family decides which accounts are even offered, so it leads. An
            undetermined family is stated outright rather than left to look like
            an unfiltered list the operator need not think about — pinning an
            account for the wrong provider is a choice that looks fine here and
            fails at the first order. */}
        <small className="form-hint">
          {chosen
            ? `${chosen.accountCode} · ${dnsFamilyLabel(chosen.dnsProvider)} · ${accountScopeLabel(chosen, t)}`
            : recordsPublishedByHand
              ? t("cloudAccountNotRequired")
              : dnsFamily === undefined
                ? declaredProvider === undefined
                  ? t("cloudAccountFamilyUnknown")
                  : t("cloudAccountFamilyUnrecognized", {
                    value: declaredProvider,
                    options: DNS_PROVIDER_SUGGESTIONS.join(" / "),
                  })
                : t("cloudAccountFamilyFiltered", { family: dnsFamilyLabel(dnsFamily) })}
        </small>
        {/* This one stays: it names where `dnsFamily` came from, which is the
            question asked the moment the list below looks shorter than
            expected, and "新增根域名" passes it too. It reports the current
            state of the filter rather than a standing rule, so it is not the
            prose that moved to the caption above. */}
        {chosen === undefined && familyNote !== undefined && <small className="form-hint">{familyNote}</small>}
      </div>
      {/* Only offered when something is actually pinned: a "use automatic"
          control on an already-automatic field is a control that does nothing,
          and on the create path there is no pin to clear at all. */}
      {value !== undefined && <button className="cloud-account-clear" type="button" onClick={() => { setAdopted(undefined); setReused(false); onChange(undefined); }}>{t("cloudAccountClear")}</button>}
    </div>
    <div className="cloud-account-actions">
      <button className="secondary-button" type="button" onClick={() => setPickerOpen(true)}>{t("cloudAccountPick")}</button>
      <button className="secondary-button" type="button" onClick={() => setRegisterOpen(true)}>{t("cloudAccountCreate")}</button>
    </div>
    {/* Offered only when the family is known and exactly one account answers for
        it: at that point opening a dialog to choose the only option is work with
        no decision in it. Nothing is pinned automatically — adopting stays an
        explicit act, because a pin freezes a choice the account center would
        otherwise keep following as the tenant's accounts change. */}
    {suggestion !== undefined && <div className="cloud-account-suggestion">
      <small className="form-hint">
        {t("cloudAccountOnlyCandidate", {
          family: dnsFamilyLabel(suggestion.dnsProvider),
          name: suggestion.displayName,
        })}
      </small>
      <button className="secondary-button" type="button" onClick={() => adopt(suggestion)}>{t("cloudAccountUse")}</button>
    </div>}
    {reused && <small className="form-hint">{t("cloudAccountReused")}</small>}
    {shownError !== undefined && <small className="form-error" role="alert">{shownError}</small>}
    {pickerOpen && <CloudAccountPickerDialog
      dnsFamily={dnsFamily}
      selectedId={value}
      t={t}
      close={() => setPickerOpen(false)}
      use={adopt}
      registerNew={() => { setPickerOpen(false); setRegisterOpen(true); }}
    />}
    {registerOpen && <CloudAccountFormDialog
      dnsFamily={dnsFamily}
      t={t}
      close={() => setRegisterOpen(false)}
      registered={(registration) => { setReused(registration.reused); adopt(registration.account); setRegisterOpen(false); }}
    />}
  </fieldset>;
}

/**
 * Picks a registered account, grouped by who can use it.
 *
 * The two groups are the distinction the server keeps and the console has to
 * show: a `tenantGlobal` account is one any member of the tenant may present
 * with, while the rest are the caller's own. Grouping is done here rather than
 * by re-sorting, so the response order — narrowest scope first, defaults ahead
 * of their siblings — stays authoritative.
 */
function CloudAccountPickerDialog({ close, dnsFamily, registerNew, selectedId, t, use }: {
  close(): void;
  dnsFamily: CloudAccountDnsProvider | undefined;
  registerNew(): void;
  selectedId: string | undefined;
  t: Translator;
  use(account: CloudAccountResponse): void;
}) {
  const service = useDeploymentsDeliveryService();
  const [family, setFamily] = useState<CloudAccountDnsProvider | "">(dnsFamily ?? "");
  const [accounts, setAccounts] = useState<CloudAccountResponse[]>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>();

  useEffect(() => {
    let active = true;
    setBusy(true); setError(undefined);
    void service.listCloudAccounts({
      page: 1,
      pageSize: 50,
      ...(family === "" ? {} : { dnsProvider: family }),
    }).then((result) => {
      if (!active) return;
      // Only accounts that can drive a DNS family are offered. An object-storage
      // account listed here would look bindable and fail at the first order.
      setAccounts(result.items.filter((account) => account.dnsProvider !== undefined));
    }).catch((cause) => { if (active) setError(errorText(cause)); })
      .finally(() => { if (active) setBusy(false); });
    return () => { active = false; };
  }, [family, service]);

  const mine = accounts.filter((account) => !account.tenantGlobal);
  const shared = accounts.filter((account) => account.tenantGlobal);

  return <Modal close={close} closeLabel={t("close")} title={t("cloudAccountPickTitle")} width="wide">
    <label className="selector-zone">
      <span>{t("dnsProvider")}</span>
      <select value={family} onChange={(event) => setFamily(event.target.value as CloudAccountDnsProvider | "")}>
        <option value="">{t("all")}</option>
        {DNS_FAMILIES.map((value) => <option key={value} value={value}>{dnsFamilyLabel(value)}</option>)}
      </select>
    </label>
    {error && <ErrorBanner message={error} t={t} />}
    {busy && <div className="empty-state">{t("loading")}</div>}
    {!busy && <>
      <CloudAccountGroup accounts={mine} label={t("cloudAccountMine")} selectedId={selectedId} t={t} use={use} />
      <CloudAccountGroup accounts={shared} label={t("cloudAccountGlobal")} selectedId={selectedId} t={t} use={use} />
      {accounts.length === 0 && <div className="empty-state">{t("cloudAccountMissing")}</div>}
    </>}
    <footer className="dialog-footer">
      <button className="secondary-button" type="button" onClick={close}>{t("cancel")}</button>
      <button className="command-button" type="button" onClick={registerNew}>{t("cloudAccountCreate")}</button>
    </footer>
  </Modal>;
}

function CloudAccountGroup({ accounts, label, selectedId, t, use }: {
  accounts: readonly CloudAccountResponse[];
  label: string;
  selectedId: string | undefined;
  t: Translator;
  use(account: CloudAccountResponse): void;
}) {
  if (accounts.length === 0) return null;
  return <fieldset className="form-fieldset">
    <legend>{label}</legend>
    <div className="hostname-selector-list">
      {accounts.map((account) => <label key={account.id}>
        <input type="radio" name="cloud-account" checked={selectedId === account.id} onChange={() => use(account)} />
        <span>
          <strong>{account.displayName}</strong>
          <small>
            {account.accountCode} · {dnsFamilyLabel(account.dnsProvider)}
            {account.isDefault ? ` · ${t("cloudAccountDefault")}` : ""}
            {/* An account with no credential behind it can be listed but not
                bound: it looks configured and is not, so the row says so rather
                than letting the operator discover it at the first order. */}
            {account.credentialConfigured ? "" : ` · ${t("cloudAccountSecret")}: ${t("pending")}`}
          </small>
        </span>
      </label>)}
    </div>
  </fieldset>;
}

/**
 * What the form asks for, decided by the family the operator picked.
 *
 * The account center stores one credential shape per family — a key pair for
 * Aliyun and DNSPod, a bearer token for Cloudflare — so the form is a projection
 * of that shape rather than a union of every family's fields. Rendering the
 * union is what made an Aliyun operator read "Aliyun AccessKeyId or DNSPod
 * LoginId" and guess which half was theirs, and what asked Cloudflare for an
 * identifier it cannot supply.
 *
 * Two things beyond the shape are decided here. The *words* are the vendor's own
 * — AccessKey ID / AccessKey Secret, Login ID / API Token, API Token — because
 * the console is meant to match the page the operator copies them from. The
 * *capability* is derived the same way the server derives it
 * (`dns_provider.vendor_code_for`), which is what lets the server refuse a
 * credential that has drifted away from the vendor it is sent for.
 */
interface CloudAccountCredentialFields {
  /** The vendor's own name for the public half; absent when the family has none. */
  identifierLabel: DeliveryMessageKey | undefined;
  identifierHint: DeliveryMessageKey | undefined;
  secretLabel: DeliveryMessageKey;
  secretHint: DeliveryMessageKey;
  /** What the operator affirms about the pair, so the server can record consent. */
  confirmsValue: DeliveryMessageKey;
}

const CLOUD_ACCOUNT_CREDENTIAL_FIELDS: Record<CloudAccountDnsProvider, CloudAccountCredentialFields> = {
  ALIYUN_DNS: {
    identifierLabel: "cloudAccountIdentifierAliyun",
    identifierHint: "cloudAccountIdentifierHint",
    secretLabel: "cloudAccountSecretAliyun",
    secretHint: "cloudAccountSecretHint",
    confirmsValue: "cloudAccountConfirmAliyun",
  },
  DNSPOD: {
    identifierLabel: "cloudAccountIdentifierDnspod",
    identifierHint: "cloudAccountIdentifierHint",
    secretLabel: "cloudAccountSecretDnspod",
    secretHint: "cloudAccountSecretHint",
    confirmsValue: "cloudAccountConfirmDnspod",
  },
  // No identifier: Cloudflare has no public half, so a field for it would be one
  // the operator could never fill correctly.
  CLOUDFLARE: {
    identifierLabel: undefined,
    identifierHint: undefined,
    secretLabel: "cloudAccountSecretCloudflare",
    secretHint: "cloudAccountSecretCloudflareHint",
    confirmsValue: "cloudAccountConfirmCloudflare",
  },
};

/**
 * Registers a cloud account from the console.
 *
 * Its environment attribute is deliberately not asked for. Deploy configures
 * credentials for certificates it issues through a publicly trusted CA, and the
 * account center's environment vocabulary (`development` / `sandbox` /
 * `production`) is not Deploy's — translating between them would be inventing a
 * mapping, so the server default is left to stand.
 */
function CloudAccountFormDialog({ close, dnsFamily, registered, t }: {
  close(): void;
  dnsFamily: CloudAccountDnsProvider | undefined;
  registered(result: CloudAccountRegistrationResponse): void;
  t: Translator;
}) {
  const service = useDeploymentsDeliveryService();
  const [displayName, setDisplayName] = useState("");
  const [accountCode, setAccountCode] = useState("");
  const [family, setFamily] = useState<CloudAccountDnsProvider>(dnsFamily ?? "ALIYUN_DNS");
  // `tenant` by default: an account registered while setting up a root domain is
  // almost always meant to serve the whole tenant, and the personal level stays
  // one click away rather than being the accidental default.
  const [scopeType, setScopeType] = useState<"tenant" | "user">("tenant");
  const [isDefault, setIsDefault] = useState(false);
  const [accessKeyId, setAccessKeyId] = useState("");
  const [secretAccessKey, setSecretAccessKey] = useState("");
  const [attested, setAttested] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>();
  const fields = CLOUD_ACCOUNT_CREDENTIAL_FIELDS[family];
  // Cloudflare has no public half, so requiring an identifier there would block
  // the one family that cannot supply one.
  const requiresIdentifier = fields.identifierLabel !== undefined;
  const identifierMissing = requiresIdentifier && accessKeyId.trim() === "";
  const incomplete = !displayName.trim() || !secretAccessKey.trim() || identifierMissing || !attested;

  async function onSubmit(event: FormEvent) {
    event.preventDefault();
    if (incomplete) return;
    setBusy(true); setError(undefined);
    try {
      registered(await service.createCloudAccount({
        displayName: displayName.trim(),
        ...(accountCode.trim() === "" ? {} : { accountCode: accountCode.trim() }),
        dnsProvider: family,
        scopeType,
        isDefault,
        ...(accessKeyId.trim() === "" ? {} : { accessKeyId: accessKeyId.trim() }),
        secretAccessKey: secretAccessKey.trim(),
        confirmsCredential: true,
      }));
    } catch (cause) { setError(errorText(cause)); setBusy(false); }
  }

  return <Modal close={close} closeLabel={t("close")} title={t("cloudAccountCreateTitle")}>
    <form onSubmit={(event) => void onSubmit(event)}>
      <div className="form-grid">
        <label><span>{t("displayName")}</span><input autoFocus required value={displayName} onChange={(event) => setDisplayName(event.target.value)} autoComplete="off" /></label>
        <label><span>{t("cloudAccountCode")}</span><input value={accountCode} onChange={(event) => setAccountCode(event.target.value)} placeholder="aliyun-dns" autoComplete="off" /></label>
        <label>
          <span>{t("dnsProvider")}</span>
          <select value={family} onChange={(event) => setFamily(event.target.value as CloudAccountDnsProvider)}>
            {DNS_FAMILIES.map((value) => <option key={value} value={value}>{dnsFamilyLabel(value)}</option>)}
          </select>
        </label>
        <label>
          <span>{t("cloudAccountScope")}</span>
          <select value={scopeType} onChange={(event) => setScopeType(event.target.value as "tenant" | "user")}>
            <option value="tenant">{t("cloudAccountScopeTenant")}</option>
            <option value="user">{t("cloudAccountScopeUser")}</option>
          </select>
        </label>
        {/* Rendered only where the family has a public half. The key is the
            family, so switching provider swaps the pair rather than leaving the
            previous vendor's words above the new vendor's field. */}
        {fields.identifierLabel !== undefined && <label className="form-field-wide">
          <span>{t(fields.identifierLabel)}</span>
          <input value={accessKeyId} onChange={(event) => setAccessKeyId(event.target.value)} autoComplete="off" aria-invalid={identifierMissing} />
          {fields.identifierHint !== undefined && <small className="form-hint">{t(fields.identifierHint)}</small>}
        </label>}
        <label className="form-field-wide">
          <span>{t(fields.secretLabel)}</span>
          <input type="password" required value={secretAccessKey} onChange={(event) => setSecretAccessKey(event.target.value)} autoComplete="new-password" />
          <small className="form-hint">{t(fields.secretHint)}</small>
        </label>
        {/* Stated rather than implied: the console cannot probe a credential, so
            the operator affirms it and the account center records who said so. */}
        <div className="form-field-wide">
          <label className="checkbox-field">
            <input type="checkbox" checked={attested} onChange={(event) => setAttested(event.target.checked)} />
            <span>{t(fields.confirmsValue)}</span>
          </label>
        </div>
        <div className="form-field-wide">
          <label className="checkbox-field">
            <input type="checkbox" checked={isDefault} onChange={(event) => setIsDefault(event.target.checked)} />
            <span>{t("cloudAccountDefault")}</span>
          </label>
        </div>
      </div>
      {error && <ErrorBanner message={error} t={t} />}
      <DialogFooter busy={busy} disabled={incomplete} close={close} submitLabel={t("create")} t={t} />
    </form>
  </Modal>;
}

/** Message per validation issue. The rule lives in `root-domain.ts`; this only names it. */
const ROOT_DOMAIN_MESSAGES: Record<RootDomainIssue, DeliveryMessageKey> = {
  required: "rootDomainRequired",
  wildcard: "rootDomainWildcard",
  malformed: "rootDomainMalformed",
  ip: "rootDomainIp",
  singleLabel: "rootDomainSingleLabel",
  tooLong: "rootDomainTooLong",
  notApex: "rootDomainNotApex",
};

/**
 * Spells a picker selection the way the wire expects it.
 *
 * The picker reports "automatic" as `undefined`, which the contract spells two
 * different ways depending on what the row looked like before: an edit that had
 * a pin and no longer does sends `""` to clear it, while a row that never had
 * one omits the key entirely. Only the caller knows which of the two it is
 * asking for, so the translation lives here rather than in the field — and the
 * create path in particular must omit, because the contract validates a supplied
 * id as a non-empty account rather than reading `""` as absent.
 */
export function pinnedAccountIdForWire(
  selected: string | undefined,
  previous: string | undefined,
): string | undefined {
  if (selected !== undefined) return selected;
  return previous === undefined ? undefined : "";
}

/**
 * Map the certificate request form onto the generated create request.
 *
 * `providerAccountId` is omitted rather than sent as an explicit `undefined`,
 * because the generated type declares it `providerAccountId?: string` and
 * `exactOptionalPropertyTypes` rejects `undefined` for that. Omission is also the
 * right wire meaning: leave the resolution chain alone, which for a certificate
 * means "adopt whatever the root domain and the account center resolve to".
 */
function toCertificateCreateRequest(body: CertificateRequestFormBody) {
  return {
    certName: body.certName,
    domainIds: body.domainIds,
    caProfile: body.caProfile,
    preferredKeyAlgorithm: body.preferredKeyAlgorithm,
    certificateScope: body.certificateScope,
    validationMethod: body.validationMethod,
    autoRenew: body.autoRenew,
    renewBeforeDays: body.renewBeforeDays,
    ...(body.providerAccountId === undefined ? {} : { providerAccountId: body.providerAccountId }),
  };
}

/**
 * Height the suggestion list is budgeted before it is rendered.
 *
 * Only the flip decision needs it, and it has to be an upper bound: a list that
 * guessed low would open downwards and then be clipped by the dialog's scrollport.
 * The row measured 35.5px in the console (8px of padding either side of a 13px
 * line box) and the list is capped by the same 216px as its own `max-height`.
 */
const SUGGEST_OPTION_HEIGHT = 37;
const SUGGEST_LIST_MAX_HEIGHT = 216;

/**
 * Free-text input with an app-rendered suggestion list.
 *
 * `<input list>` used to draw this list. It no longer does: a native datalist popup
 * is positioned by the browser rather than by the page, and in this console it
 * opened 44.7px to the right of the field and 23px wider than it. Both are read
 * from the outermost thing the browser drew for each — the input's focus ring and
 * the popup's panel — in the 1.5x capture that reported the defect, where the
 * field's border box measures 286px and the popup's panel starts 120.5 device px
 * in against the input's 53. Taking the input's border box instead of its ring
 * moves the pair to 42.7px and 27.3px; either way the offset is near half the
 * control's own width. Chromium has tracked the same class of
 * defect for that popup under zoom since 2019 (crbug 41453881), and no page-side
 * rule can move it, so the list is rendered here instead — absolutely positioned
 * inside the field's own box, which is what keeps it flush with the field at every
 * zoom and device scale factor, and inside the dialog's stacking context, so
 * nothing has to be kept in sync with a portal.
 *
 * Still an input, not a `<select>`: `deploy_dns_zone.dns_provider` is free text,
 * `manual` is a real declaration, and a vendor this build cannot name has to stay
 * expressible — hence suggestions rather than a closed set.
 */
function SuggestInput({ id, label, onChange, options, value }: {
  id: string;
  label: string;
  onChange(next: string): void;
  options: readonly string[];
  value: string;
}) {
  const [open, setOpen] = useState(false);
  const [active, setActive] = useState(-1);
  // The dialog scrolls, so a list that always opened downwards would be clipped by
  // the dialog's own scrollport whenever the field sits in its lower half. The room
  // is read from the dialog rather than from the viewport because the dialog is what
  // clips it.
  const [dropUp, setDropUp] = useState(false);
  const field = useRef<HTMLDivElement>(null);
  const listId = `${id}-list`;
  const listBudget = Math.min(options.length * SUGGEST_OPTION_HEIGHT + 8, SUGGEST_LIST_MAX_HEIGHT);

  function openList() {
    const box = field.current?.getBoundingClientRect();
    const scrollport = field.current?.closest(".dialog")?.getBoundingClientRect();
    if (box && scrollport) setDropUp(scrollport.bottom - box.bottom < listBudget + 12);
    setActive(-1);
    setOpen(true);
  }

  function commit(option: string) {
    onChange(option);
    setOpen(false);
    setActive(-1);
  }

  function onKeyDown(event: KeyboardEvent<HTMLInputElement>) {
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      if (!open) { openList(); return; }
      const step = event.key === "ArrowDown" ? 1 : -1;
      setActive((current) => (current + step + options.length) % options.length);
      return;
    }
    // Enter belongs to the open list, not to the form: without this the dialog would
    // submit the half-filled form the moment an option was highlighted.
    if (event.key === "Enter" && open && active >= 0) { event.preventDefault(); commit(options[active]!); return; }
    if (event.key === "Escape" && open) { event.preventDefault(); setOpen(false); setActive(-1); }
  }

  return <div className="suggest-field" ref={field}>
    <label htmlFor={id}>{label}</label>
    {/* The toggle is anchored to the control row, not to the field: the list is also
        a child of the field, and a toggle measured from the bottom would ride down
        with it every time the list opened. */}
    <div className="suggest-control">
      <input
        id={id}
        role="combobox"
        aria-expanded={open}
        aria-controls={listId}
        aria-autocomplete="list"
        aria-activedescendant={active >= 0 ? `${id}-option-${active}` : undefined}
        autoComplete="off"
        value={value}
        onChange={(event) => { onChange(event.target.value); if (!open) openList(); }}
        onClick={() => { if (!open) openList(); }}
        onKeyDown={onKeyDown}
        // Focus moving inside the field (to the toggle) must not close the list; only
        // leaving the field altogether does.
        onBlur={(event) => { if (!field.current?.contains(event.relatedTarget)) { setOpen(false); setActive(-1); } }}
      />
      <button
        className="suggest-toggle"
        type="button"
        tabIndex={-1}
        aria-label={label}
        aria-controls={listId}
        aria-expanded={open}
        onMouseDown={(event) => event.preventDefault()}
        onClick={() => { if (open) { setOpen(false); setActive(-1); } else { openList(); } }}
      >
        <ChevronDown size={16} />
      </button>
    </div>
    {open && <ul className="suggest-list" id={listId} role="listbox" aria-label={label} data-placement={dropUp ? "up" : "down"}>
      {options.map((option, index) => <li
        key={option}
        id={`${id}-option-${index}`}
        role="option"
        aria-selected={option === value}
        className="suggest-option"
        data-active={index === active}
        // Cancelling mousedown is what keeps the click reachable at all: letting the
        // input blur first would close the list before the click lands on it.
        onMouseDown={(event) => event.preventDefault()}
        onClick={() => commit(option)}
      >{option}</li>)}
    </ul>}
  </div>;
}

function ZoneFormDialog({ close, submit, t, zone }: {
  close(): void;
  submit(body: DomainZoneFormBody): Promise<void>;
  t: Translator;
  zone?: DomainZoneResponse | undefined;
}) {
  const [apexHostname, setApexHostname] = useState(zone?.apexHostname ?? "");
  const [displayName, setDisplayName] = useState(zone?.displayName ?? "");
  const [dnsProvider, setDnsProvider] = useState(zone?.dnsProvider ?? "");
  const [providerZoneRef, setProviderZoneRef] = useState("");
  const [providerAccountId, setProviderAccountId] = useState<string | undefined>(zone?.providerAccountId ?? undefined);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>();
  // Report as soon as the operator has typed something, never on an untouched
  // empty field: `required` already covers that case.
  const apex = validateRootDomain(apexHostname);
  const apexMessage = apexHostname.trim().length > 0 && !apex.ok ? t(ROOT_DOMAIN_MESSAGES[apex.issue]) : undefined;
  // The provider typed in this form is the one the picker filters by, so a root
  // domain being defined and its account being chosen are answered together
  // instead of the operator having to save, reopen, and pick.
  const declaredFamily = dnsFamilyFromDeclared(dnsProvider);
  // `manual` is a recognised answer, not an unreadable one. Without this the
  // "cannot drive that provider" hint would fire on a perfectly valid value and
  // tell the operator to fix something that is not broken.
  const declaredManual = providerIsManual(dnsProvider);
  async function onSubmit(event: FormEvent) {
    event.preventDefault();
    if (!apex.ok) return;
    const apexValue = apex.value;
    setBusy(true); setError(undefined);
    try {
      await submit({
        apexHostname: apexValue,
        displayName: optionalText(displayName),
        dnsProvider: optionalText(dnsProvider),
        providerZoneRef: optionalText(providerZoneRef),
        providerAccountId: pinnedAccountIdForWire(providerAccountId, zone?.providerAccountId ?? undefined),
      });
    } catch (cause) { setError(errorText(cause)); setBusy(false); }
  }
  return <Modal close={close} closeLabel={t("close")} title={zone ? t("editRootTitle") : t("createRootTitle")}>
    <form onSubmit={(event) => void onSubmit(event)}>
      {/* The root domain owns the first row: it is the only required field, and
          validation feedback plus its hint need the whole track. The provider
          zone reference is full width as well because the contract allows up
          to 512 characters. */}
      <div className="form-grid">
        {/* The root domain is frozen once the zone exists, so this is a value
            being shown rather than a field being edited. It used to be
            `disabled`, which hands both the appearance and the interaction to the
            browser: in this console the browser paints it in its own grey, and
            either way the value cannot be focused, selected, or copied — on the
            one string that says what is being edited. `readOnly` refuses the edit
            and keeps the value a value. */}
        <label className="form-field-wide">
          <span>{t("apexHostname")}</span>
          <input autoFocus={!zone} required readOnly={Boolean(zone)} value={apexHostname} onChange={(event) => setApexHostname(event.target.value)} placeholder="example.com" autoComplete="off" aria-invalid={apexMessage !== undefined} aria-describedby="delivery-apex-help" />
          <small className="form-hint" id="delivery-apex-help">{t("apexHint")}</small>
          {apexMessage !== undefined && <small className="form-error" role="alert">{apexMessage}</small>}
          {apexMessage === undefined && apex.ok && apex.converted && <small className="form-hint">{t("rootDomainConverted", { ascii: apex.value })}</small>}
        </label>
        <label><span>{t("displayName")}</span><input value={displayName} onChange={(event) => setDisplayName(event.target.value)} autoComplete="off" /></label>
        {/* No "cannot drive that provider" hint here: the account field below
            owns that sentence, because it is the one whose list stopped being
            filtered. Saying it twice would print the same warning in two
            places. */}
        <SuggestInput
          id="delivery-dns-provider"
          label={t("dnsProvider")}
          value={dnsProvider}
          onChange={setDnsProvider}
          options={DNS_PROVIDER_SUGGESTIONS}
        />
        <CloudAccountField
          value={providerAccountId}
          onChange={setProviderAccountId}
          dnsFamily={declaredFamily}
          declaredProvider={dnsProvider.trim() !== "" && declaredFamily === undefined && !declaredManual ? dnsProvider.trim() : undefined}
          familyNote={declaredFamily === undefined ? undefined : t("cloudAccountFamilyFromDeclared")}
          recordsPublishedByHand={declaredManual}
          t={t}
        />
        <label className="form-field-wide"><span>{t("providerZoneRef")}</span><input value={providerZoneRef} onChange={(event) => setProviderZoneRef(event.target.value)} autoComplete="off" /></label>
      </div>
      {error && <ErrorBanner message={error} t={t} />}
      <DialogFooter busy={busy} disabled={apexMessage !== undefined} close={close} submitLabel={zone ? t("save") : t("create")} t={t} />
    </form>
  </Modal>;
}

function DomainHostnameList({ locale }: { locale: DeploymentsLocale }) {
  const { zoneId = "" } = useParams();
  const service = useDeploymentsDeliveryService();
  const t = translator(locale);
  const [zone, setZone] = useState<DomainZoneResponse>();
  const [hostnames, setHostnames] = useState<DomainHostnameResponse[]>([]);
  const [pageInfo, setPageInfo] = useState<PageInfo>({ mode: "offset", page: 1, pageSize: 20, hasMore: false });
  const [page, setPage] = useState(1);
  const [pageSize, setPageSize] = useState(20);
  const [refreshVersion, setRefreshVersion] = useState(0);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>();
  const [createOpen, setCreateOpen] = useState(false);
  const [editTarget, setEditTarget] = useState<DomainHostnameResponse>();
  const [deleteTarget, setDeleteTarget] = useState<DomainHostnameResponse>();
  const [verification, setVerification] = useState<DomainVerifyResponse>();

  useEffect(() => {
    let active = true;
    setBusy(true); setError(undefined);
    void Promise.all([
      service.retrieveDomainZone(zoneId),
      service.listDomainHostnames(zoneId, { page, pageSize }),
    ]).then(([zoneResult, hostnameResult]) => {
      if (!active) return;
      setZone(zoneResult);
      setHostnames(hostnameResult.items);
      setPageInfo(hostnameResult.pageInfo);
    }).catch((cause) => { if (active) setError(errorText(cause)); }).finally(() => { if (active) setBusy(false); });
    return () => { active = false; };
  }, [page, pageSize, refreshVersion, service, zoneId]);

  const reload = () => setRefreshVersion((value) => value + 1);

  /**
   * 子域名台账列。rename/delete 的锁定规则来自 `rowLocks`（与向导的覆盖域名
   * 选择器共用），两张表提供同一组改名/删除动作，所以必须锁同样的行。
   */
  const hostnameColumns = useMemo<DataTableColumn<DomainHostnameResponse>[]>(() => [
    {
      id: "hostname",
      header: t("hostname"),
      cell: (hostname) => <span className="hostname-cell"><Globe2 size={16} /><strong>{hostname.hostname}</strong></span>,
      width: 260,
    },
    { id: "hostnameType", header: t("type"), cell: (hostname) => hostname.hostnameType === "WILDCARD" ? t("wildcard") : t("exact"), width: 110 },
    { id: "verificationStatus", header: t("verification"), cell: (hostname) => <StatusBadge value={hostname.verificationStatus} t={t} />, width: 130 },
    { id: "certificateCount", header: t("certificateCoverage"), cell: (hostname) => hostname.certificateCount, width: 130 },
    { id: "bindingCount", header: t("appBindings"), cell: (hostname) => hostname.bindingCount, width: 110 },
    { id: "updatedAt", header: t("updated"), cell: (hostname) => formatDate(hostname.updatedAt, locale), width: 180 },
  ], [locale, t]);

  // The loading guard sits *after* every hook, not before them. It used to sit
  // above `hostnameColumns`, which made the hook count depend on the state it
  // tests: the first render reaches here with `busy` still false (the effect
  // that flips it runs after render), so the hook ran; the render the effect
  // itself schedules arrives with `busy` true and `zone` still undefined, so
  // the early return skipped the hook — one hook fewer than the previous
  // render. React reports that as "Rendered fewer hooks than expected" and
  // unmounts the subtree, which is why opening a root domain's 子域名 list
  // crashed the page. A guard is a render decision, so it belongs with the
  // render it guards, below anything that registers a hook.
  if (!zone && busy) return <section className="resource-page"><div className="empty-state">{t("loading")}</div></section>;

  return <section className="resource-page domain-page">
    <Link className="back-link" to="/console/domains"><ArrowLeft size={16} />{t("backDomains")}</Link>
    <div className="resource-commandbar">
      <div className="resource-identity"><h1>{zone?.apexHostname ?? "-"}</h1></div>
      <div className="actions"><button className="icon-button" type="button" disabled={busy} title={t("refresh")} onClick={reload}><RefreshCw size={17} /></button><button className="command-button" type="button" onClick={() => setCreateOpen(true)}><Plus size={16} />{t("addHostname")}</button></div>
    </div>
    {zone && <div className="metric-strip">
      <Metric label={t("verification")} value={t("verifiedSummary", { verified: zone.verifiedHostnameCount, total: zone.hostnameCount })} />
      <Metric label={t("certificates")} value={zone.certificateCount} />
      <Metric label={t("appBindings")} value={zone.bindingCount} />
      <Metric label={t("status")} value={zone.status === "ACTIVE" ? t("active") : t("paused")} />
    </div>}
    {error && <ErrorBanner message={error} t={t} />}
    <DataTable<DomainHostnameResponse>
      columns={hostnameColumns}
      density="compact"
      emptyState={<span><Globe2 size={24} />{t("noHostnames")}</span>}
      getRowId={(hostname) => hostname.id}
      loading={busy && hostnames.length === 0}
      pagination={serverPagination(page, pageInfo, busy, setPage, setPageSize)}
      rowActions={(hostname) => {
        // The apex hostname row is owned by the zone itself and can only be
        // removed together with the whole zone; hostnames with active
        // certificate coverage or application bindings keep their name and
        // cannot be renamed or deleted independently.
        const locks = rowLocks(hostname, zone);
        const renameBlocked = locks.apex || locks.referenced;
        return <div className="row-actions">
          <button className="table-action" type="button" disabled={hostname.verificationStatus === "VERIFIED"} title={t("verify")} aria-label={`${t("verify")} ${hostname.hostname}`} onClick={() => { setBusy(true); void service.verifyDomainHostname(zoneId, hostname.id).then((result) => { setVerification(result); reload(); }).catch((cause) => setError(errorText(cause))).finally(() => setBusy(false)); }}><ShieldCheck size={16} /></button>
          {/* The record instructions have to be reopenable, not a one-shot dialog.
              Ownership is proven by a record the operator publishes by hand, so
              they leave this page to do it and come back to check — and the value
              they were shown once would otherwise be gone, leaving a row that can
              never be proven and therefore a certificate that can never be
              ordered. The server re-derives the value on every read, so asking
              again is a safe way to display it again. */}
          {hostname.verificationStatus !== "VERIFIED" && <button className="table-action" type="button" disabled={busy} title={t("viewRecord")} aria-label={`${t("viewRecord")} ${hostname.hostname}`} onClick={() => { setBusy(true); void service.verifyDomainHostname(zoneId, hostname.id).then((result) => setVerification(result)).catch((cause) => setError(errorText(cause))).finally(() => setBusy(false)); }}><ClipboardList size={16} /></button>}
          {hostname.verificationStatus === "VERIFIED" ? <Link className="table-action" to={`/console/certificates?zoneId=${encodeURIComponent(zoneId)}&domainId=${encodeURIComponent(hostname.id)}&hostname=${encodeURIComponent(hostname.hostname)}`} title={t("requestCertificate")} aria-label={`${t("requestCertificate")} ${hostname.hostname}`}><FileKey2 size={16} /></Link> : <button className="table-action" type="button" disabled title={t("requestCertificate")} aria-label={`${t("requestCertificate")} ${hostname.hostname}`}><FileKey2 size={16} /></button>}
          <button className="table-action" type="button" disabled={renameBlocked} title={locks.apex ? t("apexEditBlocked") : locks.referenced ? t("renameBlocked") : t("editHostname")} aria-label={`${t("editHostname")} ${hostname.hostname}`} onClick={() => setEditTarget(hostname)}><Pencil size={16} /></button>
          <button className="table-action danger-action" type="button" disabled={locks.apex || locks.referenced} title={locks.apex ? t("apexDeleteBlocked") : locks.referenced ? t("hostnameBlocked") : t("delete")} aria-label={`${t("delete")} ${hostname.hostname}`} onClick={() => setDeleteTarget(hostname)}><Trash2 size={16} /></button>
        </div>;
      }}
      rowActionsLabel={t("operations")}
      rows={hostnames}
      stickyHeader
    />
    {createOpen && <HostnameFormDialog t={t} close={() => setCreateOpen(false)} submit={async (relativeName) => { await service.createDomainHostname(zoneId, { relativeName }); setCreateOpen(false); reload(); }} />}
    {editTarget && <HostnameFormDialog hostname={editTarget} t={t} close={() => setEditTarget(undefined)} submit={async (relativeName) => { await service.updateDomainHostname(zoneId, editTarget.id, { relativeName }); setEditTarget(undefined); reload(); }} />}
    {deleteTarget && <ConfirmDialog title={t("deleteHostnameTitle")} message={t("deleteHostnameConfirm")} dangerous t={t} close={() => setDeleteTarget(undefined)} submit={async () => { await service.deleteDomainHostname(zoneId, deleteTarget.id); setDeleteTarget(undefined); reload(); }} />}
    {verification && <VerificationDialog result={verification} t={t} zoneApex={zone?.apexHostname} close={() => setVerification(undefined)} />}
  </section>;
}

/**
 * Creates a hostname, or renames one.
 *
 * `note` is an optional second sentence for the caller's own context: inside the
 * wizard's coverage picker a new name is declared the instant it is created, and
 * that is not something the rename copy can say on the ledger's behalf.
 */
function HostnameFormDialog({ close, hostname, note, submit, t }: { close(): void; hostname?: DomainHostnameResponse; note?: string; submit(relativeName: string): Promise<void>; t: Translator }) {
  const [relativeName, setRelativeName] = useState(hostname?.relativeName ?? "");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>();
  async function onSubmit(event: FormEvent) {
    event.preventDefault();
    if (!relativeName.trim()) return;
    setBusy(true); setError(undefined);
    try { await submit(relativeName.trim().toLowerCase()); } catch (cause) { setError(errorText(cause)); setBusy(false); }
  }
  return <Modal close={close} closeLabel={t("close")} title={hostname ? t("editHostnameTitle") : t("addHostnameTitle")}><form onSubmit={(event) => void onSubmit(event)}>
    <div className="form-grid single-column"><label><span>{t("relativeName")}</span><input autoFocus required value={relativeName} onChange={(event) => setRelativeName(event.target.value)} placeholder="@ / www / api.eu / *" autoComplete="off" /><small className="form-hint">{hostname ? t("renameHint") : t("relativeNameHint")}{note === undefined ? "" : ` ${note}`}</small></label></div>
    {error && <ErrorBanner message={error} t={t} />}<DialogFooter busy={busy} close={close} submitLabel={hostname ? t("save") : t("create")} t={t} />
  </form></Modal>;
}

function VerificationDialog({ close, result, t, zoneApex }: { close(): void; result: DomainVerifyResponse; t: Translator; zoneApex?: string | undefined }) {
  const [copied, setCopied] = useState<string>();
  async function copy(name: string, value: string) {
    try { await navigator.clipboard.writeText(value); setCopied(name); } catch { setCopied(undefined); }
  }
  // Providers' consoles ask for the record owner relative to the zone they
  // already know. The server resolves it authoritatively (it owns the zone
  // apex); the local fold is only a fallback for a response that predates the
  // `recordRelativeName` field, and it is not even attempted without an apex.
  const relative = result.recordRelativeName ?? (result.recordName === undefined
    ? undefined
    : relativeRecordName(result.recordName, zoneApex));
  return <Modal close={close} closeLabel={t("close")} title={t("verificationTitle")}>
    <div className={result.verified ? "verification-success" : "verification-pending"}><BadgeCheck size={19} />{result.verified ? t("verificationComplete") : t("verificationInstructions")}</div>
    {relative !== undefined && <CopyField label={t("relativeName")} value={relative} copied={copied === "relative"} copy={() => void copy("relative", relative)} t={t} hint={t("relativeNameHint")} />}
    {result.recordName && <CopyField label={t("recordName")} value={result.recordName} copied={copied === "name"} copy={() => void copy("name", result.recordName!)} t={t} />}
    {result.token && <CopyField label={t("recordValue")} value={result.token} copied={copied === "token"} copy={() => void copy("token", result.token!)} t={t} />}
    {result.expiresAt && <div className="verification-expiry"><span>{t("expiresAt")}</span><strong>{result.expiresAt}</strong></div>}
    <footer className="dialog-footer"><button className="secondary-button" type="button" onClick={close}>{t("close")}</button></footer>
  </Modal>;
}

function CopyField({ copied, copy, hint, label, t, value }: { copied: boolean; copy(): void; hint?: string | undefined; label: string; t: Translator; value: string }) {
  return <div className="copy-field"><span>{label}</span><code>{value}</code><button className="table-action" type="button" title={copied ? t("copied") : t("copy")} onClick={copy}><Clipboard size={16} /></button>{hint !== undefined && <small className="form-hint">{hint}</small>}</div>;
}

export function CertificateManagementPage({ locale }: DeploymentsResourcePageProps) {
  const service = useDeploymentsDeliveryService();
  const t = translator(locale);
  const [searchParams, setSearchParams] = useSearchParams();
  const initialDomainId = searchParams.get("domainId") ?? undefined;
  const initialHostname = searchParams.get("hostname") ?? undefined;
  const initialZoneId = searchParams.get("zoneId") ?? undefined;
  // The root-domain list links here with a root domain and nothing else, so the
  // form opens on `zoneId` alone too: the operator then picks hostnames inside
  // it. `apex` is carried purely as a label for a root domain the ACTIVE-only
  // option list may not return.
  const initialApex = searchParams.get("apex") ?? undefined;
  const [certificates, setCertificates] = useState<CertificateResponse[]>([]);
  const [pageInfo, setPageInfo] = useState<PageInfo>({ mode: "offset", page: 1, pageSize: 20, hasMore: false });
  const [page, setPage] = useState(1);
  const [pageSize, setPageSize] = useState(20);
  const [refreshVersion, setRefreshVersion] = useState(0);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>();
  const [createOpen, setCreateOpen] = useState(Boolean(initialDomainId || initialZoneId));
  const [renewTarget, setRenewTarget] = useState<CertificateResponse>();
  const [historyTarget, setHistoryTarget] = useState<CertificateResponse>();
  const [revokeTarget, setRevokeTarget] = useState<CertificateResponse>();

  useEffect(() => {
    if (initialDomainId || initialZoneId) setCreateOpen(true);
  }, [initialDomainId, initialZoneId]);

  useEffect(() => {
    let active = true;
    setBusy(true); setError(undefined);
    void service.listCertificates({ page, pageSize }).then((result) => {
      if (!active) return;
      setCertificates(result.items); setPageInfo(result.pageInfo);
    }).catch((cause) => { if (active) setError(errorText(cause)); }).finally(() => { if (active) setBusy(false); });
    return () => { active = false; };
  }, [page, pageSize, refreshVersion, service]);

  const reload = () => setRefreshVersion((value) => value + 1);
  const closeCreate = () => { setCreateOpen(false); setSearchParams({}, { replace: true }); };

  const certificateColumns = useMemo<DataTableColumn<CertificateResponse>[]>(() => [
    {
      id: "certName",
      header: t("certificates"),
      cell: (certificate) => <span className="certificate-name"><FileKey2 size={17} /><strong>{certificate.certName}</strong></span>,
      width: 220,
    },
    {
      id: "identifiers",
      header: t("identifiers"),
      cell: (certificate) => <div className="identifier-list">{certificate.identifiers.map((identifier) => <span key={identifier}>{identifier}</span>)}</div>,
      width: 280,
    },
    {
      id: "certificateScope",
      header: t("certificateType"),
      cell: (certificate) => <>{certificate.certificateScope === "WILDCARD" ? t("wildcard") : t("scopeSingleDomain")}<small className="cell-subtitle">{certificate.validationMethod === "DNS_01" ? t("validationDns01") : certificate.validationMethod === "HTTP_01" ? t("validationHttp01") : t("validationAuto")}</small></>,
      width: 150,
    },
    { id: "status", header: t("status"), cell: (certificate) => <StatusBadge value={certificate.status} t={t} />, width: 120 },
    { id: "keyAlgorithm", header: t("keyAlgorithm"), cell: (certificate) => certificate.preferredKeyAlgorithm, width: 120 },
    { id: "caProfile", header: t("caProfile"), cell: (certificate) => certificate.caProfile, width: 180 },
    { id: "validity", header: t("validity"), cell: (certificate) => <CertificateValidity certificate={certificate} locale={locale} t={t} />, width: 200 },
    { id: "renewal", header: t("renewal"), cell: (certificate) => <CertificateRenewal certificate={certificate} locale={locale} t={t} />, width: 180 },
  ], [locale, t]);

  return <section className="resource-page domain-page">
    <div className="resource-commandbar">
      <div className="resource-identity"><h1>{t("certificatesTitle")}</h1></div>
      <div className="actions"><button className="icon-button" type="button" disabled={busy} title={t("refresh")} onClick={reload}><RefreshCw size={17} /></button><button className="command-button" type="button" onClick={() => setCreateOpen(true)}><Plus size={16} />{t("requestCertificate")}</button></div>
    </div>
    {error && <ErrorBanner message={error} t={t} />}
    <DataTable<CertificateResponse>
      columns={certificateColumns}
      density="compact"
      emptyState={<span><FileKey2 size={24} />{t("noCertificates")}</span>}
      getRowId={(certificate) => certificate.id}
      loading={busy && certificates.length === 0}
      pagination={serverPagination(page, pageInfo, busy, setPage, setPageSize)}
      rowActions={(certificate) => (
        <div className="row-actions">
          <button className="table-action" type="button" disabled={certificate.certificateSource !== "MANAGED" || certificate.status === "REVOKED"} title={t("renew")} aria-label={`${t("renew")} ${certificate.certName}`} onClick={() => setRenewTarget(certificate)}><RotateCw size={16} /></button>
          <button className="table-action" type="button" title={t("renewalHistory")} aria-label={`${t("renewalHistory")} ${certificate.certName}`} onClick={() => setHistoryTarget(certificate)}><History size={16} /></button>
          <button className="table-action danger-action" type="button" disabled={certificate.status === "REVOKED"} title={t("revoke")} aria-label={`${t("revoke")} ${certificate.certName}`} onClick={() => setRevokeTarget(certificate)}><Trash2 size={16} /></button>
        </div>
      )}
      rowActionsLabel={t("operations")}
      rows={certificates}
      stickyHeader
    />
    {createOpen && <CertificateFormDialog initialTarget={initialDomainId || initialZoneId ? { domainId: initialDomainId, hostname: initialHostname, zoneId: initialZoneId, apex: initialApex } : undefined} t={t} close={closeCreate} submit={async (body) => { await service.createCertificate(toCertificateCreateRequest(body)); closeCreate(); reload(); }} />}
    {renewTarget && <ConfirmDialog title={t("renew")} message={renewTarget.certName} t={t} close={() => setRenewTarget(undefined)} submit={async () => { await service.renewCertificate(renewTarget.id); setRenewTarget(undefined); reload(); }} />}
    {historyTarget && <CertificateRenewalHistoryDialog certificate={historyTarget} locale={locale} t={t} close={() => setHistoryTarget(undefined)} />}
    {revokeTarget && <ConfirmDialog title={t("revokeCertificateTitle")} message={t("revokeCertificateConfirm")} dangerous t={t} close={() => setRevokeTarget(undefined)} submit={async () => { await service.deleteCertificate(revokeTarget.id); setRevokeTarget(undefined); reload(); }} />}
  </section>;
}

/**
 * What the caller already knows when it opens the request form.
 *
 * There are three real entry points — from a hostname row, from a root-domain
 * row, and from the toolbar with nothing preselected — so every part is
 * optional and only the parts that are present are used.
 */
interface CertificateRequestTarget {
  domainId?: string | undefined;
  hostname?: string | undefined;
  zoneId?: string | undefined;
  /** Label for a preselected root domain the ACTIVE-only option list may omit. */
  apex?: string | undefined;
}

/** Certificate scope, mirroring the contract enum. */
type CertificateScopeValue = "SINGLE_DOMAIN" | "WILDCARD";
/** Validation method, mirroring the contract enum. */
type ValidationMethodValue = "AUTO" | "HTTP_01" | "DNS_01";
type CertificateKeyAlgorithm = "RSA" | "ECDSA";
type CertificateCaProfile = "LETS_ENCRYPT_STAGING" | "LETS_ENCRYPT_PRODUCTION";

/** Payload the request form hands back to the page. */
interface CertificateRequestFormBody {
  certName: string;
  domainIds: string[];
  caProfile: CertificateCaProfile;
  preferredKeyAlgorithm: CertificateKeyAlgorithm;
  certificateScope: CertificateScopeValue;
  validationMethod: ValidationMethodValue;
  autoRenew: boolean;
  renewBeforeDays: number;
  /**
   * The account this certificate presents with, overriding its root domain's.
   *
   * Absent when the operator left it on automatic. Unlike the zone form there is
   * no clearing state to express: a certificate is created, not edited here, so
   * there is never an existing pin to remove.
   */
  providerAccountId?: string | undefined;
}

/** Contract bounds for the renewal lead time, mirroring the database constraint. */
const DEFAULT_RENEW_BEFORE_DAYS = 30;
const MINIMUM_RENEW_BEFORE_DAYS = 7;
const MAXIMUM_RENEW_BEFORE_DAYS = 90;

/**
 * Canonical ASCII hostname: lower case, no trailing dot.
 *
 * Deliberately leaves any leading `*` alone. Whether a name is a wildcard is
 * carried by `DomainHostnameResponse.hostnameType` instead, because a second
 * reader deriving it from the spelling is how `*.*.shop` got built once already.
 */
function normalizeHostname(raw: string): string {
  return raw.trim().toLowerCase().replace(/\.+$/, "");
}

/** Longest accepted hostname, mirroring the service's `hostname.len() > 253`. */
const MAX_HOSTNAME_LENGTH = 253;
/**
 * Upper bound on identifiers per certificate, mirroring the contract's
 * `MAX_CERTIFICATE_IDENTIFIERS` (which matches
 * `deploy_certificate_identifier.position BETWEEN 0 AND 99`).
 */
const MAX_CERTIFICATE_IDENTIFIERS = 100;

/**
 * Why the chosen identifier set cannot be submitted.
 *
 * The names double as message keys, so the reader reports its reason without a
 * second table to keep in step with the first. Every member is reachable from
 * the picker alone: the set is assembled by checking already-declared hostnames,
 * and what is checked can be invalidated afterwards by switching the scope above
 * it — which is why the reasons are reported here rather than prevented in the
 * dialog.
 */
export type CoverageIssue =
  | "hostnameRequired"
  | "coverageTooLong"
  | "singleDomainRejectsWildcard"
  | "coverageSingleDomainLimit"
  | "coverageIdentifierLimit"
  | "coverageWildcardRequired";

/**
 * The identifier set the server will plan for `hostnames`.
 *
 * A wildcard adds its apex because a wildcard SAN does not cover the bare name;
 * the server plans that apex in whether or not the caller asked for it
 * (`plan_certificate_identifiers`), and every planned identifier has to be a
 * claim the tenant proved. So the preview, the claim batch and the submit all
 * read this list, and none of them re-derives it.
 *
 * Takes finished hostnames rather than a relative record plus an apex: the
 * identifier set is chosen from the hostnames a root domain already declares, so
 * there is no fold left for this function to get wrong.
 */
export function planHostnames(hostnames: readonly string[]): string[] {
  const names = [...hostnames];
  for (const hostname of hostnames) {
    if (!hostname.startsWith("*.")) continue;
    const bare = hostname.slice(2);
    if (!names.includes(bare)) names.push(bare);
  }
  return names;
}

/**
 * Merges freshly ensured rows into the loaded list, so the candidate pickers and
 * the identifier preview show the claims this request just created.
 */
function mergeRows(
  current: readonly DomainHostnameResponse[],
  ensured: readonly DomainHostnameResponse[],
): DomainHostnameResponse[] {
  const byId = new Map(current.map((row) => [row.id, row]));
  for (const row of ensured) byId.set(row.id, row);
  return [...byId.values()];
}

/**
 * Keeps the published TXT value visible across re-checks.
 *
 * The proof digest is returned only by the call that created the verification
 * attempt (ADR-20260723 section 3), so a later check reports state without
 * replaying it. The operator still needs the same value in front of them to
 * publish, so the previous one is carried forward instead of the field going
 * blank while they wait for DNS to catch up.
 */
function mergeClaimsWithPublishedValue(
  previous: readonly DomainHostnameClaimResponse[],
  next: readonly DomainHostnameClaimResponse[],
): DomainHostnameClaimResponse[] {
  const published = new Map(previous.map((claim) => [claim.hostname.id, claim.dnsRecordValue]));
  return next.map((claim) => {
    if (claim.dnsRecordValue !== undefined) return claim;
    const value = published.get(claim.hostname.id);
    return value === undefined ? claim : { ...claim, dnsRecordValue: value };
  });
}

/**
 * The two reasons a declared name resists being renamed or removed.
 *
 * Both are the server's rules, spelled once: a root domain owns its own apex row
 * (`update` and `delete` both reject `@`), and a name a certificate or an
 * application already references has to be released first. Stated once so the
 * hostname ledger and the coverage picker inside the certificate wizard — the
 * two tables that offer the same rename and delete — cannot drift apart, and so
 * the reason a row looks disabled is the same sentence in both.
 */
function rowLocks(row: DomainHostnameResponse, zone: { apexHostname: string } | undefined): { apex: boolean; referenced: boolean } {
  return {
    apex: row.relativeName === "@" || (zone !== undefined && row.hostname === zone.apexHostname),
    referenced: Number(row.certificateCount) > 0 || Number(row.bindingCount) > 0,
  };
}

/** How many hostnames one read asks for, and how many reads the reader will make. */
const PICKER_PAGE_SIZE = 200;
const PICKER_PAGE_LIMIT = 5;

/**
 * Every hostname a root domain declares.
 *
 * Paged to the end rather than to a first screenful: the operator is choosing
 * from what this root domain has, so a list that stopped early would read as a
 * root domain missing names it actually holds. `PICKER_PAGE_LIMIT` bounds a list
 * that never ends; it is not a page control, because this table has no next page
 * to offer.
 *
 * Takes the one method it is allowed to reach for, so what a reader can touch is
 * part of its type rather than a convention.
 */
async function readZoneHostnames(
  service: { listDomainHostnames(zoneId: string, params?: { page?: number; pageSize?: number }): Promise<{ items: DomainHostnameResponse[]; pageInfo: PageInfo }> },
  zoneId: string,
): Promise<DomainHostnameResponse[]> {
  const collected: DomainHostnameResponse[] = [];
  for (let page = 1; page <= PICKER_PAGE_LIMIT; page += 1) {
    const result = await service.listDomainHostnames(zoneId, { page, pageSize: PICKER_PAGE_SIZE });
    collected.push(...result.items);
    if (!result.pageInfo.hasMore) break;
  }
  return collected;
}

/**
 * Whether the certificate being requested can take `row` on top of `draft`.
 *
 * A row already in the draft is never blocked: unchecking it has to stay
 * possible, and a checkbox that disabled itself once checked would be a one-way
 * door. Everything else the current certificate type cannot accept is disabled
 * rather than hidden, and creating a subdomain has to answer the same question,
 * so the rule is a function the table, the create path and the select-all header
 * all call instead of a branch inside the row markup.
 */
function rowBlockedByScope(
  draft: readonly DomainHostnameResponse[],
  row: DomainHostnameResponse,
  scope: CertificateScopeValue,
): boolean {
  if (draft.some((item) => item.hostname === row.hostname)) return false;
  if (scope === "SINGLE_DOMAIN") return draft.length >= 1 || row.hostnameType === "WILDCARD";
  return planHostnames([...draft.map((item) => item.hostname), row.hostname]).length > MAX_CERTIFICATE_IDENTIFIERS;
}

/** One root domain the picker can browse. A `DomainZoneResponse` satisfies it. */
interface PickerZone {
  id: string;
  apexHostname: string;
}

/**
 * Chooses the hostnames a certificate covers, one root domain at a time.
 *
 * The set is edited here rather than in the form because choosing it is a
 * two-part decision — which root domain, then which of its hostnames — and
 * splitting the two between a select and a checkbox list made the operator hold
 * half of it in their head while looking at the other half. The left column is
 * the root domains; the right is everything the selected one declares.
 *
 * The root domain is in that list rather than derived on top of it: a zone
 * declares its own apex when it is created (`create_zone` writes the row in the
 * same transaction), so the name is something the server reports rather than
 * something this dialog has to know how to spell.
 *
 * The selection is a draft until "confirm". A picker that applied as it was
 * checked could not be cancelled, and the point of moving the choice in here is
 * that what the form shows is a result the operator agreed to.
 *
 * Browsing another root domain clears the draft rather than adding to it — a
 * certificate covers one root domain, so a name kept from the previous one could
 * never be submitted alongside the new one, and keeping it would only look like
 * coverage that is not there.
 */
function HostnamePickerDialog({ apply, close, initialRows, initialZoneId, scope, selection, t, zones }: {
  apply(next: { zoneId: string; rows: readonly DomainHostnameResponse[]; selection: readonly DomainHostnameResponse[] }): void;
  close(): void;
  initialRows: readonly DomainHostnameResponse[];
  initialZoneId: string;
  scope: CertificateScopeValue;
  selection: readonly DomainHostnameResponse[];
  t: Translator;
  zones: readonly PickerZone[];
}) {
  const service = useDeploymentsDeliveryService();
  const [activeZoneId, setActiveZoneId] = useState(initialZoneId || zones[0]?.id || "");
  // Rows already read, by root domain. Seeded from the caller so reopening the
  // picker does not re-read the root domain the form is already showing, and
  // kept so browsing back and forth costs one request per root domain rather
  // than one per click. A ref, not state: nothing renders off the cache itself.
  const cache = useRef(new Map<string, DomainHostnameResponse[]>(
    initialZoneId !== "" && initialRows.length > 0 ? [[initialZoneId, [...initialRows]]] : [],
  ));
  const [rows, setRows] = useState<DomainHostnameResponse[]>(cache.current.get(activeZoneId) ?? []);
  const [draft, setDraft] = useState<DomainHostnameResponse[]>([...selection]);
  const [filter, setFilter] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>();
  // The record a write is aimed at. Four slots rather than one tagged union: each
  // is opened by a different control and the dialogs are disjoint in practice, so
  // a discriminant would add a variant without removing a state.
  const [createOpen, setCreateOpen] = useState(false);
  const [editTarget, setEditTarget] = useState<DomainHostnameResponse>();
  const [deleteTarget, setDeleteTarget] = useState<DomainHostnameResponse>();
  const [verification, setVerification] = useState<DomainVerifyResponse>();

  const activeZone = zones.find((zone) => zone.id === activeZoneId);

  /**
   * Adopts a list the server just reported, dragging the draft along with it.
   *
   * A rename changes the identifier a chosen row stands for and a delete takes
   * the row away, so the draft is re-pointed at what the server now returns
   * instead of keeping the copies the dialog was opened with. That is also why
   * every write re-reads rather than patching this list locally: the server's
   * order — apex first, then by name — is the order the table shows, and a row
   * patched in place would sit where it was created rather than where it sorts.
   */
  const adopt = useCallback((zoneId: string, next: DomainHostnameResponse[]) => {
    cache.current.set(zoneId, next);
    setRows(next);
    setDraft((current) => current
      .map((row) => next.find((item) => item.id === row.id))
      .filter((row): row is DomainHostnameResponse => row !== undefined));
    return next;
  }, []);

  useEffect(() => {
    if (activeZoneId === "" || cache.current.has(activeZoneId)) return;
    let active = true;
    setBusy(true);
    setError(undefined);
    void readZoneHostnames(service, activeZoneId)
      .then((items) => { if (active) adopt(activeZoneId, items); })
      .catch((cause) => { if (active) setError(errorText(cause)); })
      .finally(() => { if (active) setBusy(false); });
    return () => { active = false; };
  }, [activeZoneId, adopt, service]);

  /** Re-reads one root domain, replacing whatever the cache held for it. */
  async function reloadZone(zoneId: string): Promise<DomainHostnameResponse[]> {
    return adopt(zoneId, await readZoneHostnames(service, zoneId));
  }

  /**
   * Re-reads the root domain on screen, for the toolbar's refresh.
   *
   * The cached copy is dropped first so the read cannot be answered by it: a
   * refresh that hands back the list it already had is not a refresh.
   */
  async function refreshZone(): Promise<void> {
    setBusy(true);
    setError(undefined);
    try {
      cache.current.delete(activeZoneId);
      await reloadZone(activeZoneId);
    } catch (cause) {
      setError(errorText(cause));
    } finally {
      setBusy(false);
    }
  }

  /**
   * Checks one row's ownership.
   *
   * Unlike create, rename and delete, this has no dialog of its own to report a
   * failure into — its success path opens one — so the picker states the failure
   * itself, and re-reads afterwards because verification moves the row's badge.
   */
  async function verify(row: DomainHostnameResponse): Promise<void> {
    setBusy(true);
    setError(undefined);
    try {
      const result = await service.verifyDomainHostname(activeZoneId, row.id);
      await reloadZone(activeZoneId);
      setVerification(result);
    } catch (cause) {
      setError(errorText(cause));
    } finally {
      setBusy(false);
    }
  }

  function switchZone(id: string) {
    if (id === activeZoneId) return;
    setDraft([]);
    setFilter("");
    setActiveZoneId(id);
    setRows(cache.current.get(id) ?? []);
  }

  const chosen = new Set(draft.map((row) => row.hostname));
  // A row that cannot join the set is disabled rather than hidden: the operator
  // asked which hostnames this root domain has, and an answer that silently
  // omitted the ones the current scope cannot take would read as a root domain
  // that is missing them.
  //
  // The rules are the same ones the form reports afterwards, applied one row at
  // a time: they refuse a row that cannot be added, and leave the set's own
  // problems (a wildcard scope with no wildcard in it) to be stated once, in
  // full, rather than repeated on every row.
  const blocked = (row: DomainHostnameResponse) => rowBlockedByScope(draft, row, scope);
  const needle = normalizeHostname(filter);
  // Searched over the record's own spellings — the full name and the relative
  // name — rather than over whatever words the table happens to print, so `@`
  // finds the apex and `*` finds the wildcards in every locale.
  const listed = needle === "" ? rows : rows.filter((row) => `${row.hostname} ${row.relativeName}`.includes(needle));

  return <Modal close={close} closeLabel={t("close")} title={t("hostnamePickerTitle")} width="picker">
    {error && <ErrorBanner message={error} t={t} />}
    <div className="hostname-picker">
      <div className="hostname-picker-zones">
        <span className="hostname-picker-caption">{t("rootDomainSelect")}</span>
        {zones.map((zone) => <button
          key={zone.id}
          className="hostname-picker-zone"
          type="button"
          aria-pressed={zone.id === activeZoneId}
          onClick={() => switchZone(zone.id)}
        >
          <strong>{zone.apexHostname}</strong>
          {/* Only the root domain being browsed can carry a count: the draft is
              one root domain's worth by construction, so a number on the others
              would be a claim about a set that does not exist. */}
          {zone.id === activeZoneId && draft.length > 0 && <small>{t("hostnamePickerSelected", { count: draft.length })}</small>}
        </button>)}
        {zones.length === 0 && <div className="selector-empty">{t("noActiveRootDomain")}</div>}
      </div>
      <div className="hostname-picker-hostnames">
        <div className="hostname-picker-head">
          <span className="hostname-picker-caption">{t("hostnameCandidates")}</span>
          {/* The two controls a records table owes an operator: re-read the root
              domain, and add a name to it. Add is the secondary button rather than
              a second primary one — the dialog's primary action is the confirm in
              the footer, and two amber buttons would be two answers to what this
              dialog does. */}
          <div className="actions">
            <button className="icon-button" type="button" disabled={busy || activeZoneId === ""} title={t("refresh")} aria-label={t("refresh")} onClick={() => void refreshZone()}><RefreshCw size={16} /></button>
            <button className="secondary-button" type="button" disabled={busy || activeZoneId === ""} title={t("hostnameDeclareNote")} onClick={() => setCreateOpen(true)}><Plus size={15} />{t("addHostname")}</button>
          </div>
        </div>
        {/* Said out loud rather than left implicit: the apex row is the root
            domain itself, and an operator told to cover a root domain's hostnames
            has no way to know the root domain is among them unless the list says
            so. */}
        <small className="form-hint">{t("hostnamePickerHint")}</small>
        {/* Filtering earns its 38px only once the list is long enough to need it:
            under a screenful the search box pushes the rows it filters out of
            view, which is the opposite of what it is for. */}
        {rows.length > 8 && <div className="search-box selector-search">
          <Search size={16} />
          <input value={filter} onChange={(event) => setFilter(event.target.value)} placeholder={t("searchHostnames")} aria-label={t("searchHostnames")} />
        </div>}
        {/* A table, not a grid of cards. The same five facts sit under every name —
            which record it is, what it resolves to, whether control of it is
            proven, what already references it, and what can be done to it — and
            columns are what let an operator read down the page and compare, where a
            card per row only lets them re-read one row at a time. */}
        <DataTable<DomainHostnameResponse>
          columns={[
            // `@` is read out as the root domain rather than left as a symbol:
            // the row it labels is that name, and the operator choosing coverage
            // is the one who has to recognise it.
            { id: "relativeName", header: t("relativeName"), cell: (row) => row.relativeName === "@" ? t("apexHostname") : row.relativeName, width: 140 },
            {
              id: "hostname",
              header: t("hostnameQualified"),
              cell: (row) => <span className="hostname-cell"><Globe2 size={16} /><span><strong>{row.hostname}</strong><small className="cell-subtitle">{row.hostnameType === "WILDCARD" ? t("wildcard") : t("exact")}</small></span></span>,
              width: 280,
            },
            { id: "verificationStatus", header: t("verification"), cell: (row) => <StatusBadge value={row.verificationStatus} t={t} />, width: 130 },
            { id: "certificateCount", header: t("certificates"), cell: (row) => row.certificateCount, width: 110 },
            { id: "bindingCount", header: t("bindings"), cell: (row) => row.bindingCount, width: 110 },
          ]}
          density="compact"
          emptyState={<span>{needle === "" ? t("noHostnames") : t("hostnamePickerNoMatch")}</span>}
          // Rows the current certificate type refuses are disabled rather than
          // hidden: the operator asked which hostnames this root domain has, and
          // an answer that silently omitted the ones the current scope cannot
          // take would read as a root domain that is missing them.
          getRowProps={(row) => (blocked(row) ? { "aria-disabled": true } : undefined)}
          getRowId={(row) => row.id}
          getRowSelectionLabel={(row) => `${t("select")} ${row.hostname}`}
          loading={busy && rows.length === 0}
          onSelectedRowIdsChange={(ids) => {
            // The framework reports the whole selection, so the draft is rebuilt
            // from it rather than toggled one row at a time. Rows the current
            // scope refuses are dropped on the way in by the same rule their own
            // checkboxes use, so select-all and one click per row agree.
            const byId = new Map(listed.map((row) => [row.id, row]));
            const next: DomainHostnameResponse[] = [];
            for (const id of ids) {
              const row = byId.get(String(id));
              if (row === undefined || next.some((item) => item.id === row.id)) continue;
              if (rowBlockedByScope(next, row, scope)) continue;
              next.push(row);
            }
            setDraft(next);
          }}
          rowActions={(row) => {
            const locks = rowLocks(row, activeZone);
            return <div className="row-actions">
              {/* The three row actions a records table owes an operator: prove
                  it, rename it, remove it. Rename and delete are locked by the
                  same two rules the hostname ledger applies, and the tooltip
                  names the one in the way rather than letting the submit fail.
                  An unproven row also gets the record instructions on demand:
                  proving it means leaving to publish a TXT record, and coming
                  back has to be able to show what to publish again. */}
              <button className="table-action" type="button" disabled={busy || row.verificationStatus === "VERIFIED"} title={t("verify")} aria-label={`${t("verify")} ${row.hostname}`} onClick={() => void verify(row)}><ShieldCheck size={16} /></button>
              {row.verificationStatus !== "VERIFIED" && <button className="table-action" type="button" disabled={busy} title={t("viewRecord")} aria-label={`${t("viewRecord")} ${row.hostname}`} onClick={() => void verify(row)}><ClipboardList size={16} /></button>}
              <button className="table-action" type="button" disabled={busy || locks.apex || locks.referenced} title={locks.apex ? t("apexEditBlocked") : locks.referenced ? t("renameBlocked") : t("editHostname")} aria-label={`${t("editHostname")} ${row.hostname}`} onClick={() => setEditTarget(row)}><Pencil size={16} /></button>
              <button className="table-action danger-action" type="button" disabled={busy || locks.apex || locks.referenced} title={locks.apex ? t("apexDeleteBlocked") : locks.referenced ? t("hostnameBlocked") : t("delete")} aria-label={`${t("delete")} ${row.hostname}`} onClick={() => setDeleteTarget(row)}><Trash2 size={16} /></button>
            </div>;
          }}
          rowActionsLabel={t("operations")}
          rows={listed}
          selectable
          selectedRowIds={listed.filter((row) => chosen.has(row.hostname)).map((row) => row.id)}
          selectionBar={{ description: t("hostnamePickerCount", { count: draft.length }) }}
        />
      </div>
    </div>
    <footer className="dialog-footer">
      <span className="hostname-picker-count">{t("hostnamePickerCount", { count: draft.length })}</span>
      <button className="secondary-button" type="button" onClick={close}>{t("cancel")}</button>
      {/* Confirming an empty set is allowed and means what it says: the picker is
          how the set is changed, and removing the last name from it is a change
          the operator may make here rather than chip by chip in the form. */}
      <button className="command-button" type="button" disabled={activeZoneId === ""} onClick={() => apply({ zoneId: activeZoneId, rows, selection: draft })}>{t("confirm")}</button>
    </footer>
    {/* A name typed into a coverage picker is a name the operator wants covered,
        so a newly created one arrives checked — after the re-read, so the draft
        holds the row the server reports rather than the reply to the create. The
        form states both halves of that before the click, and the row stays
        unchecked when the certificate type on screen cannot take it. */}
    {createOpen && <HostnameFormDialog t={t} note={t("hostnameDeclareNote")} close={() => setCreateOpen(false)} submit={async (relativeName) => {
      const created = await service.createDomainHostname(activeZoneId, { relativeName });
      const next = await reloadZone(activeZoneId);
      setCreateOpen(false);
      const fresh = next.find((row) => row.id === created.id);
      if (fresh !== undefined) setDraft((current) => (current.some((item) => item.id === fresh.id) || rowBlockedByScope(current, fresh, scope) ? current : [...current, fresh]));
    }} />}
    {editTarget && <HostnameFormDialog hostname={editTarget} t={t} close={() => setEditTarget(undefined)} submit={async (relativeName) => {
      await service.updateDomainHostname(activeZoneId, editTarget.id, { relativeName });
      await reloadZone(activeZoneId);
      setEditTarget(undefined);
    }} />}
    {deleteTarget && <ConfirmDialog title={t("deleteHostnameTitle")} message={t("deleteHostnameConfirm")} dangerous t={t} close={() => setDeleteTarget(undefined)} submit={async () => {
      await service.deleteDomainHostname(activeZoneId, deleteTarget.id);
      await reloadZone(activeZoneId);
      setDeleteTarget(undefined);
    }} />}
    {verification && <VerificationDialog result={verification} t={t} zoneApex={activeZone?.apexHostname} close={() => setVerification(undefined)} />}
  </Modal>;
}

export interface CertificateFormDialogProps {
  close(): void;
  initialTarget?: CertificateRequestTarget | undefined;
  submit(body: CertificateRequestFormBody): Promise<void>;
  t: Translator;
}

// Exported so the request wizard can be mounted on its own in a browser
// harness: its scope/ownership decisions are the part most worth driving for
// real, and they are invisible to a static DOM copy.
export function CertificateFormDialog({ close, initialTarget, submit, t }: CertificateFormDialogProps) {
  const service = useDeploymentsDeliveryService();
  const [scope, setScope] = useState<CertificateScopeValue>(
    initialTarget?.hostname?.startsWith("*.") ? "WILDCARD" : "SINGLE_DOMAIN",
  );
  const [zones, setZones] = useState<DomainZoneResponse[]>([]);
  // The root domain the chosen hostnames belong to, set by the picker rather
  // than by a select of its own: a certificate covers one root domain, and that
  // root domain is chosen in the same place its hostnames are.
  const [zoneId, setZoneId] = useState(initialTarget?.zoneId ?? "");
  // The chosen identifier set, held as the declared rows themselves. One root
  // domain's worth of rows, because the picker switches rather than mixes: a
  // name folded against one apex is the wrong name against another, so a set
  // spanning two root domains could not be submitted as one certificate anyway.
  const [selectedRows, setSelectedRows] = useState<DomainHostnameResponse[]>([]);
  // The declared hostnames of the chosen root domain. Kept so the "will be
  // created" preview can tell a name that still needs declaring from one that
  // already exists, and so the picker opens on what the last claim batch left
  // behind. Written by the picker when it applies, refreshed by the claim batch.
  const [zoneHostnames, setZoneHostnames] = useState<DomainHostnameResponse[]>([]);
  const [pickerOpen, setPickerOpen] = useState(false);
  const [validationMethod, setValidationMethod] = useState<ValidationMethodValue>("AUTO");
  const [algorithm, setAlgorithm] = useState<CertificateKeyAlgorithm>("ECDSA");
  const [caProfile, setCaProfile] = useState<CertificateCaProfile>("LETS_ENCRYPT_PRODUCTION");
  // Automatic renewal is the default because the alternative is a certificate
  // that lapses while everyone assumes something else renews it. The lead time is
  // bounded here for the same reason the server bounds it: a value outside 7–90
  // is rejected by the database, and the operator should see that in the field
  // rather than as a failed request.
  const [autoRenew, setAutoRenew] = useState(true);
  const [renewBeforeDays, setRenewBeforeDays] = useState(String(DEFAULT_RENEW_BEFORE_DAYS));
  const [certNameDraft, setCertNameDraft] = useState<string>();
  // The account this certificate presents with. Undefined means automatic, which
  // is not the same as "no account": the server then walks the zone's pin and the
  // account center, and pinning here would freeze a choice the operator did not
  // make.
  const [providerAccountId, setProviderAccountId] = useState<string>();
  const [busy, setBusy] = useState(false);
  const [loadingOptions, setLoadingOptions] = useState(false);
  // Whether the root-domain list has answered. Kept apart from `loadingOptions`
  // because the wizard no longer has a hostname list of its own to wait for —
  // the picker loads whatever it browses, and this only decides when the "no
  // active root domain" hint is allowed to appear.
  const [zonesLoaded, setZonesLoaded] = useState(false);
  const [error, setError] = useState<string>();
  const [copiedField, setCopiedField] = useState<string>();
  // The claim rows this request resolved to. Held so the retry after a
  // successful ownership check submits that same set, instead of resolving a
  // second time against rows the cache still reports as pending.
  const [resolvedRows, setResolvedRows] = useState<DomainHostnameResponse[]>([]);
  // The claims still waiting for a published TXT record. They carry the record
  // to publish and the digest the server showed once, so the panel survives the
  // re-checks that follow.
  const [pendingClaims, setPendingClaims] = useState<DomainHostnameClaimResponse[]>([]);
  const claimsRef = useRef<HTMLDivElement>(null);

  // What the caller already knew, narrowed to the two lookups the seed needs:
  // the row's id (a hostname link carries it) and the name to fall back to (a
  // root-domain link carries only its apex). Both are strings, so the seed
  // effect's dependencies stay comparable across renders — `initialTarget` is a
  // fresh object on every render of the page that mounts this dialog.
  const seedDomainId = initialTarget?.domainId ?? "";
  const seedHostname = normalizeHostname(initialTarget?.hostname ?? initialTarget?.apex ?? "");
  const seedApplied = useRef(false);

  // The root domains the picker offers. Nothing is adopted from this list: the
  // root domain is a decision the picker makes, and quietly taking the first one
  // would decide the certificate's coverage and filter the account field by a
  // root domain the operator never chose.
  //
  // Root domains only, the same invariant the domain list holds. The request asks
  // for `scope=USER` and the response is filtered again, because a certificate
  // covers hostnames the operator owns: the inventory also holds the platform's
  // `app.<suffix>` zones, whose hostnames are already verified, so offering them
  // here let an operator pick one and order a certificate over the deployment's
  // own names. `zones[0]` seeds the dialog's initial root domain, so an unfiltered
  // list could also open the picker on a platform zone without anyone choosing it.
  useEffect(() => {
    let active = true;
    void service.listDomainZones({ page: 1, pageSize: 50, status: "ACTIVE", scope: "USER" })
      .then((result) => {
        if (!active) return;
        // A gateway that predates the `scope` query parameter drops it silently and
        // answers with the whole inventory, which is what put platform zones in this
        // list. The form's invariant is "the operator's own root domains", so it
        // holds that invariant rather than trusting the response to have honoured
        // the request.
        setZones(result.items.filter((zone) => zone.scope === "USER"));
      })
      .catch((cause) => { if (active) setError(errorText(cause)); })
      .finally(() => { if (active) setZonesLoaded(true); });
    return () => { active = false; };
  }, [service]);

  // Resolving the caller's target into the row it names, so a link from a
  // hostname row opens the wizard on that hostname rather than on an empty field
  // the operator has to rebuild. This is the only hostname list the form loads
  // itself: every other root domain is loaded by the picker, which is the one
  // place that browses them, and it hands back what it read.
  //
  // One-shot, keyed on a ref rather than on the seed: the operator changing the
  // root domain in the picker must not re-run the caller's seed and put the
  // wizard back on the hostname it was opened with.
  useEffect(() => {
    if (seedApplied.current || !zoneId || !seedHostname) return;
    seedApplied.current = true;
    let active = true;
    setLoadingOptions(true);
    void (async () => {
      const collected: DomainHostnameResponse[] = [];
      // The list has to be complete even here. The row the caller named is the
      // one row that matters, and a partial list would miss it and silently open
      // on an empty field — the failure this fetch exists to prevent.
      for (let page = 1; page <= 5; page += 1) {
        const result = await service.listDomainHostnames(zoneId, { page, pageSize: 200 });
        collected.push(...result.items);
        if (!result.pageInfo.hasMore) break;
      }
      return collected;
    })()
      .then((items) => {
        if (!active) return;
        setZoneHostnames(items);
        // By id first: a hostname link carries the row it came from, and an id
        // cannot be confused by a second row spelling the same name. The name
        // fallback covers a root-domain link, which carries no id.
        const row = items.find((item) => item.id === seedDomainId)
          ?? items.find((item) => item.hostname === seedHostname);
        if (row) setSelectedRows([row]);
      })
      .catch((cause) => { if (active) setError(errorText(cause)); })
      .finally(() => { if (active) setLoadingOptions(false); });
    return () => { active = false; };
  }, [seedDomainId, seedHostname, service, zoneId]);

  const selectedZone = zones.find((zone) => zone.id === zoneId);
  // The family the account picker filters by comes from the zone's own
  // declaration, so a root domain that already named its provider does not make
  // the operator restate it here. A zone that declared nothing yields no filter
  // and the picker offers every account — which the field now says out loud,
  // because an unexplained unfiltered list invites pinning an account for the
  // wrong provider, and that only surfaces when the first order fails.
  //
  // No root domain chosen yet lands in the same state as one that declared
  // nothing, and that is the honest answer: until a root domain is chosen there
  // is no zone for the server to resolve an account from, so the field is right
  // to offer every account rather than guess at one.
  const zoneDnsFamily = dnsFamilyFromDeclared(selectedZone?.dnsProvider);
  // A distinct state from "declared nothing": the record is published by hand, so
  // no account participates and the field says so instead of asking for one.
  const zoneRecordsPublishedByHand = providerIsManual(selectedZone?.dnsProvider);
  // And a third: the zone did name a provider, in a spelling this build cannot
  // read. Handed to the field rather than explained here because the field owns
  // the sentence — it is the one whose list stopped filtering — and because
  // "the provider was never decided" is simply false in this state.
  const zoneDnsProviderText = (selectedZone?.dnsProvider ?? "").trim();
  const zoneDeclaredProvider = zoneDnsProviderText !== "" && zoneDnsFamily === undefined && !zoneRecordsPublishedByHand
    ? zoneDnsProviderText
    : undefined;
  // The set that was chosen, and the set the server will plan for it. They differ
  // by each wildcard's apex, which a wildcard SAN does not cover and the server
  // adds on top (`plan_certificate_identifiers`); every planned identifier has to
  // be a proven claim. The preview, the claim batch and the submit all read the
  // planned list, and none of them re-derive it — an apex that only the submit
  // remembered is how a request gets rejected for a hostname the operator was
  // never shown.
  const coverageHostnames = selectedRows.map((row) => row.hostname);
  const plannedHostnames = planHostnames(coverageHostnames);
  // A wildcard's apex, planned by the server on top of what was chosen. Shown as
  // a derived row rather than as a removable chip, because the plan would put it
  // straight back and the operator would have removed nothing.
  const derivedHostnames = plannedHostnames.filter((hostname) => !coverageHostnames.includes(hostname));

  const declaredNames = new Set(zoneHostnames.map((row) => row.hostname));
  // Named before the submit, never after: the claim batch creates these rows, and
  // a row appearing as a side effect of a request that looked like it had failed
  // is what made adding a hostname feel like something had gone wrong.
  const hostnamesToDeclare = plannedHostnames.filter((hostname) => !declaredNames.has(hostname));

  // The set's own state. Every reason here is reachable from the picker alone:
  // the set is assembled by checking declared hostnames, and what was checked can
  // be invalidated afterwards by switching the scope above it. `hostnameRequired`
  // is the one reachable with nothing chosen, and it is rendered as a hint rather
  // than an error so a freshly opened dialog is not already complaining.
  //
  // The two single-domain rules are contract rules, reported here so the refusal
  // is a field message rather than a rejected request: a single-domain
  // certificate covers exactly one hostname, and it cannot cover a wildcard.
  const coverageIssue: CoverageIssue | undefined = (() => {
    if (plannedHostnames.length === 0) return "hostnameRequired";
    if (plannedHostnames.length > MAX_CERTIFICATE_IDENTIFIERS) return "coverageIdentifierLimit";
    if (plannedHostnames.some((hostname) => hostname.length > MAX_HOSTNAME_LENGTH)) return "coverageTooLong";
    if (scope === "SINGLE_DOMAIN" && selectedRows.length !== 1) return "coverageSingleDomainLimit";
    if (scope === "SINGLE_DOMAIN" && selectedRows.some((row) => row.hostnameType === "WILDCARD")) return "singleDomainRejectsWildcard";
    if (scope === "WILDCARD" && !selectedRows.some((row) => row.hostnameType === "WILDCARD")) return "coverageWildcardRequired";
    return undefined;
  })();

  // A wildcard can only be authorized over DNS-01, so the form shows the method
  // the order will actually use rather than keeping a preference it cannot.
  const effectiveValidationMethod: ValidationMethodValue =
    scope === "WILDCARD" ? "DNS_01" : validationMethod;

  // Bounded here exactly as the server bounds it, so an out-of-range lead time is
  // caught in the field instead of returning as a rejected request that names a
  // constraint. Checked on digit strings rather than on `Number(...)` because the
  // empty box and `1e2` both coerce to a number the operator did not type.
  //
  // Validated whether or not automatic renewal is on: the lead time is a stored
  // property of the certificate, not a mode of the scheduler, and it stays in
  // effect the moment renewal is switched on.
  const renewBeforeDaysIssue: DeliveryMessageKey | undefined = (() => {
    const trimmed = renewBeforeDays.trim();
    if (!/^[0-9]+$/.test(trimmed)) return "renewBeforeDaysInvalid";
    const days = Number(trimmed);
    return days < MINIMUM_RENEW_BEFORE_DAYS || days > MAXIMUM_RENEW_BEFORE_DAYS
      ? "renewBeforeDaysInvalid"
      : undefined;
  })();

  // Named after what the certificate leads with: the wildcard base it is built
  // around, or the single hostname it covers. Derived from the chosen set, so it
  // stops being a name the operator has to keep in step with the hostnames.
  const nameSource = scope === "WILDCARD"
    ? selectedRows.find((row) => row.hostnameType === "WILDCARD")
    : selectedRows[0];
  const derivedCertName = nameSource === undefined
    ? ""
    : scope === "WILDCARD"
      ? `${nameSource.hostname.slice(2)} ${t("wildcard")}`
      : nameSource.hostname;
  const certName = certNameDraft ?? derivedCertName;

  // What the picker's left column lists: every ACTIVE root domain, plus the one
  // the caller linked from when it is not among them. A link from a paused root
  // domain still has to say where the current selection lives, or the picker
  // would open looking as if it had lost it.
  const pickerZones = zoneId !== "" && !zones.some((zone) => zone.id === zoneId)
    ? [{ id: zoneId, apexHostname: initialTarget?.apex ?? zoneId }, ...zones]
    : zones;

  async function copyValue(key: string, value: string) {
    try {
      await navigator.clipboard.writeText(value);
      setCopiedField(key);
    } catch {
      setCopiedField(undefined);
    }
  }

  function resetOwnershipPanel() {
    setPendingClaims([]);
    setResolvedRows([]);
  }

  /**
   * Takes the picker's result as the identifier set.
   *
   * The whole result at once, not one name at a time: the picker is the only
   * place the set is edited, and it is also the only place that knows which root
   * domain the names came from. A set from another root domain replaces the
   * previous one rather than merging with it — a certificate covers one root
   * domain, and a quiet merge would submit names the operator never saw.
   *
   * `rows` is what the picker read for that root domain, kept because the "will
   * be created" preview has to tell a name that still needs declaring from one
   * that already exists. The picker loads the complete list to show it, so this
   * costs no extra request.
   */
  function applyHostnameSelection(next: {
    zoneId: string;
    rows: readonly DomainHostnameResponse[];
    selection: readonly DomainHostnameResponse[];
  }) {
    resetOwnershipPanel();
    setZoneId(next.zoneId);
    setZoneHostnames([...next.rows]);
    setSelectedRows([...next.selection]);
    setPickerOpen(false);
  }

  /** Drops one hostname from the set, by name: the chips carry no other handle. */
  function removeSelected(hostname: string) {
    resetOwnershipPanel();
    setSelectedRows((current) => current.filter((row) => row.hostname !== hostname));
  }

  async function submitWith(rows: readonly DomainHostnameResponse[]): Promise<void> {
    await submit({
      certName: certName.trim(),
      domainIds: rows.map((row) => row.id),
      caProfile,
      preferredKeyAlgorithm: algorithm,
      certificateScope: scope,
      validationMethod: effectiveValidationMethod,
      autoRenew,
      renewBeforeDays: Number(renewBeforeDays.trim()),
      ...(providerAccountId === undefined ? {} : { providerAccountId }),
    });
  }

  /**
   * Decides between submitting and asking for the proof, from what the ownership
   * endpoint actually observed.
   *
   * The same decision serves the first submit and the "check now" retry: asking
   * for a check *is* the observation, so a claim that verifies on the first
   * attempt submits immediately instead of showing a panel for a problem that is
   * already solved.
   */
  async function settleOwnership(
    claims: readonly DomainHostnameClaimResponse[],
    all: readonly DomainHostnameResponse[],
  ): Promise<void> {
    const outstanding = claims.filter((claim) => !claim.verified);
    if (outstanding.length > 0) {
      setPendingClaims((current) => mergeClaimsWithPublishedValue(current, outstanding));
      setBusy(false);
      return;
    }
    // The cached claim list still reports these rows as pending, and the retry
    // path reads claim state from it, so mark them locally as well.
    const verified = new Set(claims.map((claim) => claim.hostname.id));
    setZoneHostnames((current) => current.map((row) =>
      verified.has(row.id) ? { ...row, verificationStatus: "VERIFIED" } : row,
    ));
    resetOwnershipPanel();
    await submitWith(all);
  }

  /**
   * Asks the ownership endpoint to make sure the scope's hostnames are claimed,
   * then settles the result.
   *
   * The server folds the fully-qualified names into zone-relative claims and
   * checks all of them in one pass, so submitting costs one round trip instead of
   * one per name - and the fold, which is zone-local and easy to get wrong, lives
   * in exactly one place.
   *
   * A certificate may only cover hostnames the tenant has proven control of, so
   * an unverified claim cannot be issued over. Ownership is a durable, expiring,
   * worker-checked attempt: the operator publishes the record and asks for a
   * check, and nothing is lost by stopping here - the retry reuses the same rows.
   */
  async function resolveClaims(): Promise<void> {
    if (!zoneId || plannedHostnames.length === 0) return;
    // The planned set, not the requested one: a wildcard's apex is planned on top
    // of what was requested, and it has to be claimed too or the order is refused
    // for a hostname that never appeared in the form.
    const claims = (await service.ensureDomainHostnameClaims(zoneId, { hostnames: plannedHostnames })).items;
    const rows = claims.map((claim) => claim.hostname);
    setResolvedRows(rows);
    setZoneHostnames((current) => mergeRows(current, rows));
    await settleOwnership(claims, rows);
  }

  async function onSubmit(event: FormEvent) {
    event.preventDefault();
    if (!zoneId || coverageIssue !== undefined || !certName.trim()) return;
    setBusy(true);
    setError(undefined);
    resetOwnershipPanel();
    try {
      await resolveClaims();
    } catch (cause) {
      setError(errorText(cause));
      setBusy(false);
    }
  }

  async function recheck() {
    if (!zoneId) return;
    setBusy(true);
    setError(undefined);
    try {
      // The same endpoint answers a re-check: it reports what DNS says right now,
      // which is the only thing that can turn an outstanding claim into proof.
      const claims = (await service.ensureDomainHostnameClaims(zoneId, {
        hostnames: pendingClaims.map((claim) => claim.hostname.hostname),
      })).items;
      setZoneHostnames((current) => mergeRows(current, claims.map((claim) => claim.hostname)));
      await settleOwnership(claims, resolvedRows);
    } catch (cause) {
      setError(errorText(cause));
      setBusy(false);
    }
  }

  // The panel sits below the pinned footer, so without this a failed ownership
  // check looks like a click that did nothing.
  useEffect(() => {
    if (pendingClaims.length > 0) claimsRef.current?.scrollIntoView({ block: "center" });
  }, [pendingClaims]);

  // `coverageIssue` already carries every reason the set cannot be submitted, so
  // the button reads one authority rather than re-listing the same conditions.
  const submitDisabled = busy
    || !zoneId
    || coverageIssue !== undefined
    || renewBeforeDaysIssue !== undefined
    || !certName.trim();

  return <Modal close={close} closeLabel={t("close")} title={t("requestCertificateTitle")} width="wide">
    <form onSubmit={(event) => void onSubmit(event)}>
      {/* The scope drives everything else: it decides how many identifiers the
          certificate plans, whether a wildcard's apex is added on top of the
          chosen names, and which validation methods stay legal. */}
      <fieldset className="form-fieldset">
        <legend>{t("certificateType")}</legend>
        <div className="segmented-control" aria-label={t("certificateType")}>
          {(["SINGLE_DOMAIN", "WILDCARD"] as const).map((value) => <button key={value} type="button" aria-pressed={scope === value} onClick={() => { setScope(value); resetOwnershipPanel(); }}>{value === "SINGLE_DOMAIN" ? t("scopeSingleDomain") : t("wildcard")}</button>)}
        </div>
        <small className="form-hint">{scope === "WILDCARD" ? t("scopeWildcardHint") : t("scopeSingleDomainHint")}</small>
      </fieldset>
      {zonesLoaded && zones.length === 0 && <small className="form-hint">{t("noActiveRootDomain")}</small>}

      {/* What the certificate covers, chosen in one place. The root domain is part
          of that choice rather than a field of its own: a certificate covers one
          root domain's hostnames, and the picker is where both are picked. The
          summary below is the result, not a second editor — every change to the
          set goes through the dialog, which is what makes the result reviewable
          before it is submitted. */}
      <fieldset className="form-fieldset">
        <legend>{t("identifiers")}</legend>
        <div className="hostname-summary">
          <button className="secondary-button" type="button" onClick={() => setPickerOpen(true)}><Globe2 size={15} />{t("coveragePick")}</button>
          {/* Which root domain the set belongs to is stated here because it is a
              consequence of the choice. A select of its own would be a second
              answer to the same question — the state this field was rebuilt to
              remove. Before anything is chosen there is no root domain, and the
              line says so rather than naming whichever one the server listed
              first, which is a root domain the operator never chose. */}
          <small className="form-hint">
            {selectedZone === undefined
              ? t("coverageRootDomainUnset")
              : t("coverageRootDomain", { apex: selectedZone.apexHostname })}
          </small>
        </div>
        {/* What the certificate will actually cover, which is not the same list as
            the chosen one: a wildcard plans its apex in as well. It sits directly
            under the trigger that opens the picker, both because that is where the
            question is asked and because the pinned footer would otherwise cover
            it at the dialog's initial scroll position. */}
        {selectedRows.length === 0
          ? <small className="form-hint">{loadingOptions ? t("loading") : t("hostnameRequired")}</small>
          : <>
            <div className="selected-hostnames">
              {/* The retention rule — the renewal lead time and the per-root-domain
                  rate limit — is reference material, not a decision the operator
                  makes here. It rides on the summary as a hover so the row stays
                  one line tall. */}
              <span title={t("coverageRetention")}>{t("coverageTitle", { count: plannedHostnames.length })}</span>
              <div>
                {selectedRows.map((row) => <span key={row.hostname}>
                  <strong>{row.hostname}</strong>
                  <small>{row.hostnameType === "WILDCARD" ? t("wildcard") : t("exact")}</small>
                  <button type="button" title={t("delete")} aria-label={`${t("delete")} ${row.hostname}`} onClick={() => removeSelected(row.hostname)}><X size={13} /></button>
                </span>)}
                {derivedHostnames.map((hostname) => <span key={hostname}>
                  <strong>{hostname}</strong>
                  <small>{t("coverageIncludedApex")}</small>
                </span>)}
              </div>
            </div>
            {coverageIssue !== undefined && <small className="form-error" role="alert">{t(coverageIssue)}</small>}
          </>}
        {/* Declaring is a write the operator authorizes here rather than discovers
            afterwards: the claim batch creates these rows, so naming them before
            the submit is what keeps a new row from looking like a side effect of
            a request that failed. */}
        {hostnamesToDeclare.length > 0 && <small className="form-hint">
          {t("coverageWillDeclare", { count: hostnamesToDeclare.length, hostnames: hostnamesToDeclare.join(", ") })}
        </small>}
      </fieldset>

      {/* The validation method and the account that will publish the record are one
          decision — DNS-01 is exactly the case that needs an account, and `AUTO`
          may resolve to it — so they share a row rather than taking two. The
          account stays visible for every method instead of being hidden for
          HTTP-01: a wildcard shown as HTTP-01-ineligible is precisely the case
          where the operator needs to see which account will answer. */}
      <div className="dialog-pair dialog-pair-settings">
        <fieldset className="form-fieldset">
          {/* Which method a scope ends up with is a consequence of the scope
              chosen above, so it belongs on the caption as a hover rather than
              as a line that restates a decision already made. */}
          <legend title={scope === "WILDCARD" ? t("validationWildcardRequiresDns01") : t("validationAutoHint")}>{t("validationMethod")}</legend>
          <div className="segmented-control" aria-label={t("validationMethod")}>
            {(["AUTO", "DNS_01", "HTTP_01"] as const).map((value) => {
              const label = value === "AUTO" ? t("validationAuto") : value === "DNS_01" ? t("validationDns01") : t("validationHttp01");
              return <button key={value} type="button" disabled={scope === "WILDCARD" && value === "HTTP_01"} aria-pressed={effectiveValidationMethod === value} onClick={() => setValidationMethod(value)}>{label}</button>;
            })}
          </div>
        </fieldset>

        {/* The family is not chosen here — it follows the root domain this
            certificate is being issued for, which is where DNS-01 has to publish.
            The field states that filter itself, so no second sentence naming the
            same source is passed in: two lines about one filter is how a summary
            becomes a paragraph. */}
        <CloudAccountField
          value={providerAccountId}
          onChange={setProviderAccountId}
          dnsFamily={zoneDnsFamily}
          declaredProvider={zoneDeclaredProvider}
          recordsPublishedByHand={zoneRecordsPublishedByHand}
          t={t}
        />
      </div>

      {/* Three columns over two rows: the two controls that decide how the
          certificate is issued, then the three that decide what it is called and
          how it is renewed. Each row is only as tall as its tallest control, so
          the reference prose that used to sit under every field — how the name is
          derived, what the lead time does to a short-lived certificate — moves to
          the caption's `title`. The field and its value stay on screen; the
          explanation is one hover away instead of one line taller. */}
      <div className="form-grid certificate-form-grid">
        <fieldset className="form-fieldset">
          <legend>{t("keyAlgorithm")}</legend>
          <div className="segmented-control algorithm-control">
            {(["RSA", "ECDSA"] as const).map((value) => <button key={value} type="button" aria-pressed={algorithm === value} onClick={() => setAlgorithm(value)}>{value === "RSA" ? t("rsa") : t("ecdsa")}</button>)}
          </div>
        </fieldset>
        <label>
          <span>{t("caProfile")}</span>
          <select value={caProfile} onChange={(event) => setCaProfile(event.target.value as CertificateCaProfile)}><option value="LETS_ENCRYPT_PRODUCTION">{t("production")}</option><option value="LETS_ENCRYPT_STAGING">{t("staging")}</option></select>
        </label>
        <label className="form-field-wide">
          <span title={t("certNameDerived")}>{t("certName")}</span>
          <input value={certName} onChange={(event) => setCertNameDraft(event.target.value)} autoComplete="off" title={t("certNameDerived")} />
        </label>
        {/* The two renewal controls stay together: the lead time is only meaningful
            next to whether renewal runs at all, and neither belongs inside a
            `fieldset` whose legend would repeat the checkbox's own label. */}
        <div className="form-field-wide">
          <label className="checkbox-field" title={t("autoRenewHint")}>
            <input type="checkbox" checked={autoRenew} onChange={(event) => setAutoRenew(event.target.checked)} />
            <span>{t("autoRenew")}</span>
          </label>
        </div>
        <label>
          <span title={t("renewBeforeDaysHint")}>{t("renewBeforeDays")}</span>
          <input type="number" inputMode="numeric" min={MINIMUM_RENEW_BEFORE_DAYS} max={MAXIMUM_RENEW_BEFORE_DAYS} step={1} value={renewBeforeDays} onChange={(event) => setRenewBeforeDays(event.target.value)} aria-invalid={renewBeforeDaysIssue !== undefined} title={t("renewBeforeDaysHint")} />
          {renewBeforeDaysIssue !== undefined && <small className="form-error" role="alert">{t(renewBeforeDaysIssue)}</small>}
        </label>
      </div>

      {pendingClaims.length > 0 && <div className="verification-pending" ref={claimsRef}>
        <div>
          <strong>{t("ownershipRequiredTitle")}</strong>
          <small className="form-hint">{t("ownershipRequiredHint")}</small>
          {pendingClaims.map((claim, index) => {
            const row = claim.hostname;
            // The server resolves the provider-facing host record (it owns the
            // zone); the local fold is only a fallback for a response that
            // predates `dnsRecordRelativeName`.
            const relative = claim.dnsRecordRelativeName ?? (claim.dnsRecordName === undefined
              ? undefined
              : relativeRecordName(claim.dnsRecordName, selectedZone?.apexHostname));
            return <div key={row.id}>
              <small className="form-hint">{row.hostname}</small>
              {relative !== undefined && <CopyField label={t("relativeName")} value={relative} copied={copiedField === `relative-${index}`} copy={() => void copyValue(`relative-${index}`, relative)} t={t} hint={t("relativeNameHint")} />}
              {claim.dnsRecordName && <CopyField label={t("recordName")} value={claim.dnsRecordName} copied={copiedField === `name-${index}`} copy={() => void copyValue(`name-${index}`, claim.dnsRecordName!)} t={t} />}
              {claim.dnsRecordValue && <CopyField label={t("recordValue")} value={claim.dnsRecordValue} copied={copiedField === `token-${index}`} copy={() => void copyValue(`token-${index}`, claim.dnsRecordValue!)} t={t} />}
            </div>;
          })}
          <small className="form-hint">{t("ownershipWillSubmit")}</small>
          <button className="command-button" type="button" disabled={busy} onClick={() => void recheck()}>{busy ? t("ownershipChecking") : t("ownershipCheckNow")}</button>
        </div>
      </div>}

      {error && <ErrorBanner message={error} t={t} />}
      <DialogFooter busy={busy} disabled={submitDisabled} close={close} submitLabel={t("requestCertificate")} t={t} />
    </form>
    {/* The picker is a sibling of the form, not a descendant of it. A dialog opened
        from inside a form brings its own <form> — the record dialog does — and a
        nested form is invalid HTML: the parser keeps both elements, but the inner
        form's submit button then submits the outer one, so confirming a new record
        navigated the whole page instead of creating it. Anything that mounts a form
        of its own belongs outside this one. */}
    {pickerOpen && <HostnamePickerDialog
      zones={pickerZones}
      initialZoneId={zoneId}
      initialRows={zoneHostnames}
      scope={scope}
      selection={selectedRows}
      t={t}
      close={() => setPickerOpen(false)}
      apply={applyHostnameSelection}
    />}
  </Modal>;
}

function ConfirmDialog({ close, dangerous = false, message, submit, t, title }: { close(): void; dangerous?: boolean; message: string; submit(): Promise<void>; t: Translator; title: string }) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>();
  async function run() {
    setBusy(true); setError(undefined);
    try { await submit(); } catch (cause) { setError(errorText(cause)); setBusy(false); }
  }
  return <Modal close={close} closeLabel={t("close")} title={title}><p className={dangerous ? "confirmation-message dangerous-confirmation" : "confirmation-message"}>{message}</p>{error && <ErrorBanner message={error} t={t} />}<footer className="dialog-footer"><button className="secondary-button" type="button" onClick={close}>{t("cancel")}</button><button className={dangerous ? "danger-button" : "command-button"} type="button" disabled={busy} onClick={() => void run()}>{t("confirm")}</button></footer></Modal>;
}

/**
 * `width` exists because the wide surfaces disagree about what they contain: a
 * form reads well at 860px, a six-column ledger inherits the table's 980px
 * minimum and would otherwise be reachable only by scrolling sideways, and the
 * hostname picker holds two columns of names side by side.
 */
const MODAL_WIDTH_CLASSES = { default: "", wide: " delivery-dialog-wide", history: " delivery-dialog-history", picker: " delivery-dialog-picker" };

function Modal({ children, close, closeLabel, title, width = "default" }: { children: ReactNode; close(): void; closeLabel: string; title: string; width?: keyof typeof MODAL_WIDTH_CLASSES }) {
  return <div className="dialog-backdrop" role="presentation" onMouseDown={(event) => { if (event.target === event.currentTarget) close(); }}><div className={`dialog delivery-dialog${MODAL_WIDTH_CLASSES[width]}`} role="dialog" aria-modal="true" aria-labelledby="delivery-dialog-title"><header><h2 id="delivery-dialog-title">{title}</h2><button className="icon-button" type="button" title={closeLabel} onClick={close}><X size={18} /></button></header>{children}</div></div>;
}

function DialogFooter({ busy, close, disabled = false, submitLabel, t }: { busy: boolean; close(): void; disabled?: boolean; submitLabel: string; t: Translator }) {
  return <footer className="dialog-footer"><button className="secondary-button" type="button" onClick={close}>{t("cancel")}</button><button className="command-button" type="submit" disabled={busy || disabled}>{submitLabel}</button></footer>;
}

/**
 * Builds the framework pagination descriptor for a server-paginated ledger.
 *
 * All three ledgers on this page read `PageInfo` from the same service, so the
 * mapping lives here once instead of three times. Optional fields are *omitted*
 * rather than set to `undefined`: the framework keys on the presence of
 * `hasMore` to switch into cursor mode and on `rowCount` to switch into offset
 * mode, and an explicit `undefined` is rejected under
 * `exactOptionalPropertyTypes`.
 *
 * ⚠️ `pageSize` and `onPageSizeChange` must travel together. The framework
 * renders the rows-per-page selector whenever `pageSizeOptions` has more than
 * one entry (it always does here — `pageSizeOptions` defaults to
 * `LEDGER_PAGE_SIZES`), but it can only *apply* a change through
 * `onPageSizeChange`. Omitting the handler leaves a selector that opens, shows
 * its options, and then silently discards the pick: `handlePageSizeChange`
 * calls `pagination.onPageSizeChange?.(nextPageSize)`, the optional call is a
 * no-op, and because `pagination.pageSize` stays defined the composite never
 * falls back to its own internal state either — so the trigger snaps back to
 * the old value as though nothing was clicked.
 */
function serverPagination(
  page: number,
  pageInfo: PageInfo,
  busy: boolean,
  setPage: (value: number | ((current: number) => number)) => void,
  onPageSizeChange: (pageSize: number) => void,
  pageSizeOptions: readonly number[] = LEDGER_PAGE_SIZES,
): DataTablePaginationProps {
  const rowCount = pageInfo.totalItems === undefined ? undefined : Number(pageInfo.totalItems);
  const pageSize = pageInfo.pageSize ?? LEDGER_PAGE_SIZES[0];
  return {
    mode: "server",
    onPageChange: (next: number) => {
      if (busy || next < 1) return;
      setPage(next);
    },
    onPageSizeChange: (next: number) => {
      if (busy || next === pageSize) return;
      onPageSizeChange(next);
      // A larger page can leave the current page past the end of the result
      // set, so changing the page size always restarts from the first page.
      setPage(1);
    },
    ...(pageInfo.hasMore === undefined ? {} : { hasMore: pageInfo.hasMore }),
    page: rowCount === undefined ? page : (pageInfo.page ?? page),
    pageSize,
    pageSizeOptions,
    ...(rowCount === undefined || !Number.isFinite(rowCount) ? {} : { rowCount }),
  };
}

/** 台账每页条数选项；服务端分页，页码由 `Pagination` 描述符驱动。 */
const LEDGER_PAGE_SIZES = [20, 50, 100] as const;

function Metric({ label, value }: { label: string; value: string }) {
  return <div className="domain-metric"><span>{label}</span><strong>{value}</strong></div>;
}

function StatusBadge({ t, value }: { t: Translator; value: string }) {
  const labels: Partial<Record<string, DeliveryMessageKey>> = { ACTIVE: "active", PAUSED: "paused", PENDING: "pending", VERIFIED: "verified", FAILED: "failed", EXPIRED: "expired", ISSUING: "issuing" };
  return <span className={`status-badge status-${value.toLowerCase()}`}>{labels[value] ? t(labels[value]!) : value}</span>;
}

/**
 * Label and colour for each validity phase the contract can report.
 *
 * The tone is drawn from the existing `status-*` palette rather than a parallel
 * `validity-*` one, so the phase reads in the same colour language as every
 * other badge on the page and no new stylesheet rule can be missed. A phase this
 * build does not know about shows its raw value in the neutral tone instead of
 * silently borrowing a colour that means something else.
 */
const VALIDITY_PHASE_KEYS: Partial<Record<string, { label: DeliveryMessageKey; tone: string }>> = {
  NOT_YET_VALID: { label: "validityNotYetValid", tone: "status-pending" },
  VALID: { label: "validityValid", tone: "status-active" },
  EXPIRING_SOON: { label: "validityExpiringSoon", tone: "status-pending" },
  EXPIRED: { label: "expired", tone: "status-expired" },
};

/** Label for each renewal status a certificate itself can carry. */
const RENEWAL_STATUS_KEYS: Partial<Record<string, DeliveryMessageKey>> = {
  NONE: "renewalIdle",
  PLANNED: "renewalPlanned",
  PROCESSING: "renewalProcessing",
  FAILED: "failed",
};

/**
 * The validity window of one certificate, as an operator needs to read it.
 *
 * The phase and the countdown are derived by the server from the version being
 * served, never stored: a persisted phase is wrong from the instant the clock
 * crosses a boundary and nothing would correct it. Showing them beside the raw
 * window is the point — a lone `notAfter` makes the only question that matters,
 * "how long have I got", into date arithmetic the operator should not be doing.
 */
function CertificateValidity({ certificate, locale, t }: { certificate: CertificateResponse; locale: DeploymentsLocale; t: Translator }) {
  const phase = certificate.validityPhase;
  const days = certificate.daysUntilExpiry;
  const described = phase ? VALIDITY_PHASE_KEYS[phase] : undefined;
  return <>
    {phase && <span className={`status-badge ${described?.tone ?? "status-paused"}`}>{described ? t(described.label) : phase}</span>}
    <small className="cell-subtitle">{certificate.notBefore ? `${t("validFrom")} ${formatDate(certificate.notBefore, locale)}` : t("validityUnknown")}</small>
    <small className="cell-subtitle">{certificate.notAfter ? `${t("expiresAt")} ${formatDate(certificate.notAfter, locale)}` : "-"}</small>
    {days !== undefined && <small className="cell-subtitle">{days >= 0 ? t("daysRemaining", { days }) : t("daysOverdue", { days: -days })}</small>}
  </>;
}

/**
 * When renewal will run, and what the last attempt did.
 *
 * `renewalDueAt` comes from the server rather than being recomputed here so the
 * console cannot describe a different window than the scheduler acts on. The
 * failure count is shown for the same reason the backoff exists: retries are
 * spaced out, so without it a certificate failing every day looks exactly like
 * one that has never been tried.
 */
function CertificateRenewal({ certificate, locale, t }: { certificate: CertificateResponse; locale: DeploymentsLocale; t: Translator }) {
  const statusKey = RENEWAL_STATUS_KEYS[certificate.renewalStatus];
  return <>
    <span>{statusKey ? t(statusKey) : certificate.renewalStatus}</span>
    <small className="cell-subtitle">{certificate.autoRenew ? `${t("autoRenew")} · ${t("renewBeforeDays")} ${certificate.renewBeforeDays}` : t("autoRenewOff")}</small>
    {certificate.autoRenew && certificate.renewalDueAt && <small className="cell-subtitle">{t("renewalDueAt")} {formatDate(certificate.renewalDueAt, locale)}</small>}
    {certificate.lastRenewalAt && <small className="cell-subtitle">{t("lastRenewedAt")} {formatDate(certificate.lastRenewalAt, locale)}</small>}
    {certificate.renewalFailureCount > 0 && <span className="status-badge status-failed">{t("renewalFailures", { count: certificate.renewalFailureCount })}</span>}
  </>;
}

/** Label, tone and explanation for each state a renewal attempt can be in. */
const RENEWAL_ATTEMPT_KEYS: Partial<Record<string, { label: DeliveryMessageKey; tone: string; detail: DeliveryMessageKey }>> = {
  PLANNED: { label: "renewalPlanned", tone: "status-pending", detail: "renewalPlannedDetail" },
  ORDERED: { label: "renewalOrdered", tone: "status-pending", detail: "renewalOrderedDetail" },
  SUCCEEDED: { label: "renewalSucceeded", tone: "status-active", detail: "renewalSucceededDetail" },
  FAILED: { label: "failed", tone: "status-failed", detail: "renewalFailedDetail" },
  SKIPPED: { label: "renewalSkipped", tone: "status-paused", detail: "renewalSkippedDetail" },
  CANCELLED: { label: "renewalCancelled", tone: "status-paused", detail: "renewalCancelledDetail" },
};

/**
 * The renewal ledger for one certificate.
 *
 * This is the only place a replaced window still exists. Once a renewal
 * succeeds the certificate row mirrors the new version's dates and the previous
 * version is superseded, so "was coverage continuous across the handover" is
 * answerable from these rows and nowhere else — which is why the replaced and
 * issued windows are shown side by side rather than just the outcome.
 *
 * The trigger is shown for a related reason: a scheduled failure is automation
 * retrying on its own, while a manual one is an act the operator is already
 * watching, and the two read very differently in a list of failures.
 *
 * Rows are rendered in the order the endpoint returns them (newest first). No
 * client-side re-sort: a reversal here would paper over a server ordering
 * mistake rather than surface it.
 */
function CertificateRenewalHistoryDialog({ certificate, locale, t, close }: { certificate: CertificateResponse; locale: DeploymentsLocale; t: Translator; close(): void }) {
  const service = useDeploymentsDeliveryService();
  const [attempts, setAttempts] = useState<CertificateRenewalResponse[]>([]);
  const [busy, setBusy] = useState(true);
  const [error, setError] = useState<string>();
  useEffect(() => {
    let active = true;
    setBusy(true); setError(undefined);
    void service.listCertificateRenewals(certificate.id, { page: 1, pageSize: 50 }).then((result) => {
      if (active) setAttempts(result.items);
    }).catch((cause) => {
      if (active) setError(errorText(cause));
    }).finally(() => {
      if (active) setBusy(false);
    });
    return () => { active = false; };
  }, [certificate.id, service]);

  return <Modal close={close} closeLabel={t("close")} title={`${t("renewalHistoryTitle")} · ${certificate.certName}`} width="history">
    {error && <ErrorBanner message={error} t={t} />}
    <DataTable<CertificateRenewalResponse>
      columns={[
        {
          id: "attemptNo",
          header: t("renewalAttempt"),
          cell: (attempt) => <>#{attempt.attemptNo}<small className="cell-subtitle">{formatDate(attempt.scheduledAt, locale)}</small></>,
          width: 150,
        },
        {
          id: "status",
          header: t("status"),
          cell: (attempt) => {
            const described = RENEWAL_ATTEMPT_KEYS[attempt.status];
            return <><span className={`status-badge ${described?.tone ?? "status-paused"}`}>{described ? t(described.label) : attempt.status}</span>{attempt.finishedAt && <small className="cell-subtitle">{t("renewalFinishedAt")} {formatDate(attempt.finishedAt, locale)}</small>}</>;
          },
          width: 200,
        },
        { id: "triggerKind", header: t("renewalTrigger"), cell: (attempt) => attempt.triggerKind === "SCHEDULED" ? t("renewalTriggerScheduled") : t("renewalTriggerManual"), width: 130 },
        { id: "previous", header: t("renewalWindowReplaced"), cell: (attempt) => <>{attempt.previousNotBefore ? formatDate(attempt.previousNotBefore, locale) : "-"}<small className="cell-subtitle">{attempt.previousNotAfter ? formatDate(attempt.previousNotAfter, locale) : "-"}</small></>, width: 190 },
        { id: "issued", header: t("renewalWindowIssued"), cell: (attempt) => <>{attempt.newNotBefore ? formatDate(attempt.newNotBefore, locale) : "-"}<small className="cell-subtitle">{attempt.newNotAfter ? formatDate(attempt.newNotAfter, locale) : "-"}</small></>, width: 190 },
        {
          id: "outcome",
          header: t("renewalOutcome"),
          cell: (attempt) => {
            const described = RENEWAL_ATTEMPT_KEYS[attempt.status];
            return <>{described ? t(described.detail) : attempt.status}{attempt.lastErrorCode && <small className="cell-subtitle">{t("renewalErrorCode")} {attempt.lastErrorCode}</small>}</>;
          },
          width: 220,
        },
      ]}
      density="compact"
      emptyState={<span>{t("renewalHistoryEmpty")}</span>}
      getRowId={(attempt) => attempt.id}
      loading={busy && attempts.length === 0}
      rows={attempts}
      stickyHeader
    />
    <footer className="dialog-footer"><button className="secondary-button" type="button" onClick={close}>{t("close")}</button></footer>
  </Modal>;
}

function ErrorBanner({ message, t }: { message?: string; t: Translator }) {
  return <div className="error-banner" role="alert">{message || t("error")}</div>;
}

function errorText(cause: unknown): string | undefined {
  return cause instanceof Error && cause.message ? cause.message : undefined;
}

function translator(locale: DeploymentsLocale): Translator {
  return (key, values) => deliveryText(locale, key, values);
}

function formatDate(value: string, locale: DeploymentsLocale): string {
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? value : new Intl.DateTimeFormat(locale, { dateStyle: "medium", timeStyle: "short" }).format(date);
}

function optionalText(value: string): string | undefined {
  return value.trim() || undefined;
}
