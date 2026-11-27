import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import {
  Activity,
  AlertTriangle,
  ArrowLeft,
  ArrowUpRight,
  Clock,
  Copy,
  Database,
  Gauge,
  Globe2,
  Link2,
  Package,
  RefreshCw,
  Rocket,
  Settings2,
  ShieldCheck,
  Wrench,
} from "lucide-react";
import {
  Badge,
  Button,
  DataTable,
  DescriptionDetails,
  DescriptionItem,
  DescriptionList,
  DescriptionTerm,
  EmptyState,
  Progress,
  StatCard,
  StatusBadge,
  StatusNotice,
  Tabs,
  TabsList,
  TabsTrigger,
  type DataTableColumn,
  type StatusBadgeVariant,
} from "@sdkwork/ui-pc-react";

import {
  TrafficTrendChart,
  formatTrendValue,
  peakTrendPoint,
  sumTrend,
  type TrafficTrendMetric,
} from "../components/UsageTrendChart";
import { useApplicationDetailMessages } from "../i18n";
import type {
  ApplicationDeliveryDetail,
  ApplicationDeploymentRecord,
  ApplicationReleaseRecord,
  DeployApplicationDetail,
} from "../types/application-detail";
import type { ApplicationDetailMessages } from "../types/application-detail-messages";
import type {
  ApplicationStorageQuota,
  ApplicationTrafficByHostname,
  ApplicationUsageAvailabilityReason,
  ApplicationUsageLedgerEntry,
  ApplicationUsageSnapshot,
  ApplicationUsageSourceFailure,
  ApplicationUsageWindow,
} from "../types/application-usage";
import { APPLICATION_USAGE_WINDOWS } from "../types/application-usage";
import type { DeployApplicationStatus } from "../types/deploy-application-kind";
import { classifyReason } from "../services/application-usage";
import {
  formatByteSize,
  formatDateTime,
  formatRatio,
  formatUsageCount,
  formatUsageMeasure,
  formatUsageWindowRange,
  formatUtcDay,
  resolveOperatorLocale,
  usageRatio,
} from "../services/application-usage-format";
import { templateMessage } from "../services/message-template";
import type {
  DeploymentApplicationDetailCapabilityPort,
  DeploymentApplicationDetailDataSource,
  DeploymentApplicationDetailPermissionPort,
} from "../services/ports";

/**
 * Deployment-application detail surface.
 *
 * The page is the read-and-understand counterpart to a deployment list, and it is
 * organized the way a cloud console organizes one resource:
 *
 * 1. **identity** — what the application is, who owns it, and which decimal id
 *    identity provisioning anchors on;
 * 2. **consumption** — traffic, service/API calls, and storage, each on its own
 *    metered plane, with the raw ledger behind the aggregates;
 * 3. **delivery** — publish readiness, publishing hostnames, and the release and
 *    deployment history that explains what is actually live.
 *
 * Everything console-specific arrives through props:
 * {@link DeploymentApplicationDetailDataSource} performs the reads with the
 * consumer's own SDK client, {@link DeploymentApplicationDetailPermissionPort}
 * reports what the operator may open, and
 * {@link DeploymentApplicationDetailCapabilityPort} opens the host-owned
 * commands. The page therefore carries no navigation, no route, no permission
 * constant, and no SDK client of its own, which is what lets a second console
 * reuse it unchanged.
 *
 * Every region loads independently. A meter that is not wired in this
 * installation renders as "not connected" with its reason — never as a zero.
 */
export function DeploymentApplicationDetailPage({
  appId,
  backTo = "",
  capabilities,
  dataSource,
  onBack,
  permissions,
}: {
  /** Deploy application uuid (`AppResponse.id`) — never the decimal id. */
  appId: string;
  /** Value the copy-link action puts on the clipboard; the host supplies it. */
  backTo?: string;
  /** Host-owned commands the header opens. */
  capabilities?: DeploymentApplicationDetailCapabilityPort;
  /** The reads, backed by the consumer's injected generated SDK client. */
  dataSource: DeploymentApplicationDetailDataSource;
  /** Host navigation back to the list that opened this page. */
  onBack?: () => void;
  /** What the operator may open; absent means "not declared". */
  permissions?: DeploymentApplicationDetailPermissionPort;
}) {
  const messages = useApplicationDetailMessages();
  const dataSourceRef = useRef(dataSource);
  dataSourceRef.current = dataSource;
  const locale = useMemo(() => resolveOperatorLocale(), []);

  const canReadSource = permissions?.sourceRead ? permissions.sourceRead() : true;

  const [detailState, setDetailState] = useState<
    | { status: "loading" }
    | { error: unknown; status: "error" }
    | { delivery: ApplicationDeliveryDetail; detail: DeployApplicationDetail; status: "ready" }
  >({ status: "loading" });
  const [usageState, setUsageState] = useState<
    | { status: "loading" }
    | { error: unknown; status: "error" }
    | {
      failures: readonly ApplicationUsageSourceFailure[];
      snapshot: ApplicationUsageSnapshot;
      status: "ready";
    }
  >({ status: "loading" });
  const [reloadToken, setReloadToken] = useState(0);
  const [usageWindow, setUsageWindow] = useState<ApplicationUsageWindow>("7d");
  const [copied, setCopied] = useState<string | null>(null);

  // The fact sheet is one read per application (and per refresh).
  useEffect(() => {
    let cancelled = false;
    setDetailState({ status: "loading" });
    void dataSourceRef.current
      .loadDetail({ appId })
      .then(({ delivery, detail }) => {
        if (!cancelled) {
          setDetailState({ delivery, detail, status: "ready" });
        }
      })
      .catch((error: unknown) => {
        if (!cancelled) {
          setDetailState({ error, status: "error" });
        }
      });
    return () => {
      cancelled = true;
    };
  }, [appId, reloadToken]);

  // Usage is a second, independent read: it covers a selected window, so it
  // re-queries when the window or the refresh changes, not when the fact sheet
  // does.
  const detailHostnames = useMemo(
    () =>
      detailState.status === "ready"
        ? detailState.delivery.domains.items.map((domain) => domain.hostname).filter(Boolean)
        : [],
    [detailState],
  );
  const loadUsage = dataSource.loadUsage;
  useEffect(() => {
    const read = dataSourceRef.current.loadUsage;
    if (!read) {
      setUsageState({
        error: Object.assign(new Error("usage not connected"), { status: 501 }),
        status: "error",
      });
      return;
    }
    let cancelled = false;
    setUsageState({ status: "loading" });
    void read({ appId, hostnames: detailHostnames, window: usageWindow })
      .then((result) => {
        if (!cancelled) {
          setUsageState({ ...result, status: "ready" });
        }
      })
      .catch((error: unknown) => {
        if (!cancelled) {
          setUsageState({ error, status: "error" });
        }
      });
    return () => {
      cancelled = true;
    };
  }, [appId, detailHostnames, loadUsage, reloadToken, usageWindow]);

  const copyText = useCallback((key: string, value: string) => {
    if (typeof navigator === "undefined" || !navigator.clipboard?.writeText) {
      return;
    }
    void navigator.clipboard
      .writeText(value)
      .then(() => {
        setCopied(key);
        globalThis.setTimeout(() => setCopied(null), 2000);
      })
      .catch(() => undefined);
  }, []);

  const header = (
    <header className="flex flex-wrap items-start justify-between gap-3">
      <div className="flex min-w-0 items-start gap-3">
        {onBack ? (
          <Button aria-label={messages.back} onClick={onBack} size="sm" type="button" variant="ghost">
            <ArrowLeft className="h-4 w-4" />
          </Button>
        ) : null}
        <div className="min-w-0">
          <div className="flex flex-wrap items-center gap-2">
            <h1 className="truncate text-xl font-semibold text-[var(--sdk-color-text-primary)]">
              {detailState.status === "ready" ? detailState.detail.name : messages.title}
            </h1>
            {detailState.status === "ready" ? (
              <StatusBadge
                label={appStatusLabel(messages, detailState.detail.appStatus)}
                showIcon
                status={detailState.detail.appStatus.toLowerCase()}
                variant={appStatusVariant(detailState.detail.appStatus)}
              />
            ) : null}
          </div>
          <p className="truncate text-sm text-[var(--sdk-color-text-muted)]">
            {detailState.status === "ready"
              ? templateMessage(messages.subtitle, {
                id: detailState.detail.deployApplicationId ?? detailState.detail.id,
                kind: appKindLabel(messages, detailState.detail.appKind),
              })
              : messages.title}
          </p>
        </div>
      </div>
      <div className="flex flex-wrap items-center gap-2">
        {backTo ? (
          <Button onClick={() => copyText("page-link", backTo)} type="button" variant="outline">
            <Link2 className="h-4 w-4" />
            {copied === "page-link" ? messages.actions.copied : messages.actions.copyLink}
          </Button>
        ) : null}
        <Button
          aria-busy={detailState.status === "loading" || undefined}
          disabled={detailState.status === "loading"}
          onClick={() => setReloadToken((token) => token + 1)}
          type="button"
          variant="outline"
        >
          <RefreshCw className={`h-4 w-4 ${detailState.status === "loading" ? "animate-spin" : ""}`} />
          {messages.refresh}
        </Button>
        {detailState.status === "ready" ? (
          <DetailActionGroup
            appId={appId}
            appKind={detailState.detail.appKind}
            capabilities={capabilities}
            canReadSource={canReadSource}
            messages={messages}
          />
        ) : null}
      </div>
    </header>
  );

  if (detailState.status === "error") {
    const reason = classifyReason(detailState.error);
    return (
      <div className="flex h-full min-h-0 flex-col gap-5" data-deployment-application-detail={appId}>
        {header}
        <StatusNotice tone="danger">
          <span className="flex flex-wrap items-center gap-3">
            <span>
              {reason === "forbidden"
                ? messages.loadErrorForbidden
                : reason === "unavailable"
                  ? messages.loadErrorUnavailable
                  : reason === "not-found"
                    ? messages.notFound
                    : messages.loadError}
            </span>
            <Button
              onClick={() => setReloadToken((token) => token + 1)}
              size="sm"
              type="button"
              variant="outline"
            >
              {messages.retry}
            </Button>
          </span>
        </StatusNotice>
      </div>
    );
  }

  if (detailState.status === "loading") {
    return (
      <div className="flex h-full min-h-0 flex-col gap-5" data-deployment-application-detail={appId}>
        {header}
        <div aria-hidden="true" className="grid gap-4 md:grid-cols-2 xl:grid-cols-4">
          {[0, 1, 2, 3].map((index) => (
            <div
              className="h-28 animate-pulse rounded-[var(--sdk-radius-panel)] border border-[var(--sdk-color-border-default)] bg-[var(--sdk-color-surface-panel-muted)]"
              key={index}
            />
          ))}
        </div>
      </div>
    );
  }

  const { delivery, detail } = detailState;
  const usage = usageState.status === "ready" ? usageState.snapshot : undefined;

  return (
    <div className="flex h-full min-h-0 flex-col gap-5" data-deployment-application-detail={appId}>
      {header}

      <ApplicationAttributeStrip detail={detail} messages={messages} />

      <div className="grid min-h-0 flex-1 gap-4 xl:grid-cols-[minmax(0,1fr)_minmax(320px,380px)]">
        <div className="min-w-0 space-y-4">
          <UsageSummaryCards
            loading={usageState.status === "loading"}
            locale={locale}
            messages={messages}
            snapshot={usage}
          />
          <UsagePlaneSection
            loading={usageState.status === "loading"}
            locale={locale}
            messages={messages}
            onRefresh={() => setReloadToken((token) => token + 1)}
            onWindowChange={setUsageWindow}
            reloadError={usageState.status === "error" ? usageState.error : undefined}
            snapshot={usage}
            window={usageWindow}
          />
          <DeliveryHistorySection delivery={delivery} messages={messages} />
        </div>
        <aside className="min-w-0 space-y-4">
          <ReadinessPanel delivery={delivery} messages={messages} />
          <DomainsPanel copied={copied} delivery={delivery} messages={messages} onCopy={copyText} />
          <StoragePanel locale={locale} messages={messages} snapshot={usage} />
        </aside>
      </div>
    </div>
  );
}

/* ------------------------------- header strip ------------------------------ */

function ApplicationAttributeStrip({
  detail,
  messages,
}: {
  detail: DeployApplicationDetail;
  messages: ApplicationDetailMessages;
}) {
  const copy = messages.overview;
  const organizationId =
    detail.organizationId && detail.organizationId !== "0" ? detail.organizationId : undefined;
  const revisionPending =
    Boolean(detail.desiredRevisionId) && detail.desiredRevisionId !== detail.currentRevisionId;
  return (
    <section
      aria-label={copy.title}
      className="rounded-[var(--sdk-radius-panel)] border border-[var(--sdk-color-border-default)] bg-[var(--sdk-color-surface-panel)] p-4"
      data-application-attributes
    >
      <DescriptionList columns={3}>
        <DescriptionItem>
          <DescriptionTerm>{copy.fields.applicationId}</DescriptionTerm>
          <DescriptionDetails mono>
            <span className="select-all break-all">{detail.deployApplicationId ?? "—"}</span>
          </DescriptionDetails>
        </DescriptionItem>
        <DescriptionItem>
          <DescriptionTerm>{copy.fields.slug}</DescriptionTerm>
          <DescriptionDetails mono>{detail.slug ?? "—"}</DescriptionDetails>
        </DescriptionItem>
        <DescriptionItem>
          <DescriptionTerm>{copy.fields.kind}</DescriptionTerm>
          <DescriptionDetails>{appKindLabel(messages, detail.appKind)}</DescriptionDetails>
        </DescriptionItem>
        <DescriptionItem>
          <DescriptionTerm>{copy.fields.ownership}</DescriptionTerm>
          <DescriptionDetails>
            <span className="flex flex-wrap items-center gap-2">
              <Badge variant="secondary">{ownershipLabel(detail, messages)}</Badge>
              {organizationId ? (
                <span className="text-xs text-[var(--sdk-color-text-muted)]">#{organizationId}</span>
              ) : null}
            </span>
          </DescriptionDetails>
        </DescriptionItem>
        <DescriptionItem>
          <DescriptionTerm>{copy.fields.environment}</DescriptionTerm>
          <DescriptionDetails>{detail.defaultEnvironment ?? "—"}</DescriptionDetails>
        </DescriptionItem>
        <DescriptionItem>
          <DescriptionTerm>{copy.fields.platformTargets}</DescriptionTerm>
          <DescriptionDetails>{detail.platformTargetCount ?? "—"}</DescriptionDetails>
        </DescriptionItem>
        <DescriptionItem>
          <DescriptionTerm>{copy.fields.latestRelease}</DescriptionTerm>
          <DescriptionDetails>{detail.latestReleaseTag ?? "—"}</DescriptionDetails>
        </DescriptionItem>
        <DescriptionItem>
          <DescriptionTerm>{copy.fields.revision}</DescriptionTerm>
          <DescriptionDetails>
            {revisionPending ? (
              <span className="flex items-center gap-1.5 text-[var(--sdk-color-state-warning)]">
                <AlertTriangle className="h-3.5 w-3.5" />
                {copy.fields.revisionPending}
              </span>
            ) : (
              <span className="font-mono text-xs">{shortId(detail.currentRevisionId) ?? "—"}</span>
            )}
          </DescriptionDetails>
        </DescriptionItem>
        <DescriptionItem>
          <DescriptionTerm>{copy.fields.webConfig}</DescriptionTerm>
          <DescriptionDetails>
            <Badge variant={detail.nginxConfigOverridden ? "warning" : "outline"}>
              {detail.nginxConfigOverridden
                ? copy.fields.webConfigOverridden
                : copy.fields.webConfigGenerated}
            </Badge>
          </DescriptionDetails>
        </DescriptionItem>
        <DescriptionItem>
          <DescriptionTerm>{copy.fields.createdAt}</DescriptionTerm>
          <DescriptionDetails>{formatDateTime(detail.createdAt)}</DescriptionDetails>
        </DescriptionItem>
        <DescriptionItem>
          <DescriptionTerm>{copy.fields.updatedAt}</DescriptionTerm>
          <DescriptionDetails>{formatDateTime(detail.updatedAt)}</DescriptionDetails>
        </DescriptionItem>
        <DescriptionItem>
          <DescriptionTerm>{copy.fields.tenant}</DescriptionTerm>
          <DescriptionDetails mono>{detail.tenantId ?? "—"}</DescriptionDetails>
        </DescriptionItem>
      </DescriptionList>
    </section>
  );
}

/* ------------------------------ usage summary ----------------------------- */

function UsageSummaryCards({
  loading,
  locale,
  messages,
  snapshot,
}: {
  loading: boolean;
  locale: string | undefined;
  messages: ApplicationDetailMessages;
  snapshot?: ApplicationUsageSnapshot | undefined;
}) {
  const copy = messages.usage;
  const traffic = snapshot?.traffic;
  const service = snapshot?.service;
  const storage = snapshot?.storage;
  const totalStorage = storage?.items.reduce(
    (total, item) => total + Number(item.usedBytes || 0),
    0,
  );
  const tenantApi = service?.usage.apiAttribution === "tenant";
  return (
    <section aria-label={copy.title} className="grid gap-4 sm:grid-cols-2 xl:grid-cols-4">
      <StatCard
        description={copy.kpi.egressHint}
        icon={<ArrowUpRight className="h-5 w-5" />}
        label={copy.kpi.egress}
        value={
          loading
            ? "…"
            : traffic?.availability !== "available" || !traffic.measured
              ? messages.usage.noData
              : formatUsageMeasure(traffic.totals.egressBytes?.quantity, "BYTE", locale)
        }
      />
      <StatCard
        description={copy.kpi.requestsHint}
        icon={<Globe2 className="h-5 w-5" />}
        label={copy.kpi.requests}
        value={
          loading
            ? "…"
            : traffic?.availability !== "available" || !traffic.measured
              ? messages.usage.noData
              : formatUsageCount(traffic.totals.requests?.quantity, locale)
        }
      />
      <StatCard
        change={tenantApi ? copy.kpi.apiCallsTenant : undefined}
        description={copy.kpi.apiCallsHint}
        icon={<Activity className="h-5 w-5" />}
        label={copy.kpi.apiCalls}
        value={
          loading
            ? "…"
            : service?.availability !== "available"
              ? reasonText(copy.unavailable, service?.availabilityReason)
              : formatUsageCount(service?.usage.apiRequests?.quantity, locale)
        }
      />
      <StatCard
        description={copy.kpi.storageHint}
        icon={<Database className="h-5 w-5" />}
        label={copy.kpi.storage}
        value={
          loading
            ? "…"
            : storage?.availability !== "available"
              ? reasonText(copy.unavailable, storage?.availabilityReason)
              : formatByteSize(String(totalStorage), locale)
        }
      />
    </section>
  );
}

/* -------------------------------- usage planes ----------------------------- */

function UsagePlaneSection({
  loading,
  locale,
  messages,
  onRefresh,
  onWindowChange,
  reloadError,
  snapshot,
  window,
}: {
  loading: boolean;
  locale: string | undefined;
  messages: ApplicationDetailMessages;
  onRefresh: () => void;
  onWindowChange: (window: ApplicationUsageWindow) => void;
  reloadError: unknown;
  snapshot?: ApplicationUsageSnapshot | undefined;
  window: ApplicationUsageWindow;
}) {
  const copy = messages.usage;
  const [metric, setMetric] = useState<TrafficTrendMetric>("requests");
  const traffic = snapshot?.traffic;
  const trend = traffic?.trend ?? [];
  const peak = peakTrendPoint(trend, metric);
  const trendTotal = sumTrend(trend, metric);
  const byHostname = traffic?.byHostname ?? [];
  const totalEgress = traffic?.totals.egressBytes?.quantity;
  const ledger = snapshot?.ledger ?? [];

  const hostnameColumns: DataTableColumn<ApplicationTrafficByHostname>[] = [
    {
      cell: (row) => (
        <span className="block max-w-[260px] truncate font-mono text-xs" title={row.hostname}>
          {row.hostname}
        </span>
      ),
      header: copy.hostnames.hostname,
      id: "hostname",
    },
    {
      align: "right",
      cell: (row) => formatUsageCount(row.requests, locale),
      header: copy.hostnames.requests,
      id: "requests",
    },
    {
      align: "right",
      cell: (row) => formatByteSize(row.egressBytes, locale),
      header: copy.hostnames.egress,
      id: "egress",
    },
    {
      align: "right",
      cell: (row) => formatRatio(usageRatio(row.egressBytes, totalEgress), locale),
      header: copy.hostnames.share,
      id: "share",
    },
  ];

  const ledgerColumns: DataTableColumn<ApplicationUsageLedgerEntry>[] = [
    {
      cell: (row) => (
        <BriefCell
          compact
          primary={dimensionLabel(copy.ledger.dimensions, row.dimension)}
          secondary={row.dimension}
        />
      ),
      header: copy.ledger.dimension,
      id: "dimension",
    },
    {
      align: "right",
      cell: (row) => formatUsageCount(row.quantity, locale),
      header: copy.ledger.quantity,
      id: "quantity",
    },
    { cell: (row) => row.unit || "—", header: copy.ledger.unit, id: "unit" },
    {
      cell: (row) => formatDateTime(row.periodStart ?? row.observedAt ?? row.createdAt, locale),
      header: copy.ledger.period,
      id: "period",
    },
    {
      cell: (row) => (
        <span
          className="block max-w-[220px] truncate font-mono text-[11px] text-[var(--sdk-color-text-muted)]"
          title={row.deduplicationKey ?? ""}
        >
          {row.deduplicationKey ?? "—"}
        </span>
      ),
      header: copy.ledger.source,
      id: "source",
    },
  ];

  return (
    <section
      aria-label={copy.title}
      className="space-y-4 rounded-[var(--sdk-radius-panel)] border border-[var(--sdk-color-border-default)] bg-[var(--sdk-color-surface-panel)] p-4"
      data-application-usage
    >
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div className="min-w-0">
          <h2 className="flex items-center gap-2 text-base font-semibold text-[var(--sdk-color-text-primary)]">
            <Gauge className="h-4 w-4" />
            {copy.title}
          </h2>
          <p className="text-sm text-[var(--sdk-color-text-muted)]">{copy.description}</p>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <Tabs
            onValueChange={(value) => onWindowChange(value as ApplicationUsageWindow)}
            value={window}
          >
            <TabsList aria-label={copy.window}>
              {APPLICATION_USAGE_WINDOWS.map((option) => (
                <TabsTrigger
                  className="min-w-0 px-3"
                  data-usage-window={option}
                  key={option}
                  value={option}
                >
                  {copy.windows[option]}
                </TabsTrigger>
              ))}
            </TabsList>
          </Tabs>
          <Button
            aria-busy={loading || undefined}
            disabled={loading}
            onClick={onRefresh}
            size="sm"
            type="button"
            variant="outline"
          >
            <RefreshCw className={`h-3.5 w-3.5 ${loading ? "animate-spin" : ""}`} />
            {copy.refresh}
          </Button>
        </div>
      </div>

      <div className="flex flex-wrap items-center gap-2 text-xs text-[var(--sdk-color-text-muted)]">
        <Badge variant="outline">
          {templateMessage(copy.windowRange, {
            range: formatUsageWindowRange(snapshot?.windowStart, snapshot?.windowEnd, locale),
          })}
        </Badge>
        {snapshot ? (
          <Badge variant={snapshot.traffic.attributed ? "success" : "warning"}>
            {snapshot.traffic.attributed ? copy.attribution.application : copy.attribution.tenant}
          </Badge>
        ) : null}
        {traffic ? (
          <Badge variant={traffic.completeness === "complete" ? "outline" : "warning"}>
            {copy.completeness[traffic.completeness]}
          </Badge>
        ) : null}
        {snapshot && snapshot.sources.length > 0 ? (
          <span data-usage-sources>
            {copy.sources}: {snapshot.sources.join(" · ")}
          </span>
        ) : null}
      </div>

      {snapshot && !snapshot.traffic.attributed ? (
        <StatusNotice tone="warning">{copy.attribution.tenantHint}</StatusNotice>
      ) : null}
      {traffic?.completeness === "partial" ? (
        <StatusNotice tone="warning">{copy.completeness.partialHint}</StatusNotice>
      ) : null}
      {reloadError ? (
        <StatusNotice tone="danger">{reasonText(copy.unavailable, classifyReason(reloadError))}</StatusNotice>
      ) : null}

      <div className="grid gap-4 lg:grid-cols-2">
        <div className="space-y-3">
          <div className="flex flex-wrap items-center justify-between gap-2">
            <h3 className="text-sm font-semibold text-[var(--sdk-color-text-primary)]">
              {copy.trend.title}
            </h3>
            <Tabs onValueChange={(value) => setMetric(value as TrafficTrendMetric)} value={metric}>
              <TabsList aria-label={copy.trend.title}>
                <TabsTrigger data-trend-metric="requests" value="requests">
                  {copy.trend.requests}
                </TabsTrigger>
                <TabsTrigger data-trend-metric="egressBytes" value="egressBytes">
                  {copy.trend.egress}
                </TabsTrigger>
              </TabsList>
            </Tabs>
          </div>
          <p className="text-xs text-[var(--sdk-color-text-muted)]">{copy.trend.description}</p>
          {trend.length === 0 ? (
            <EmptyState description={copy.trend.empty} />
          ) : (
            <div className="space-y-2" data-usage-trend-chart>
              <TrafficTrendChart metric={metric} points={trend} />
              <div className="flex flex-wrap items-center gap-4 text-xs text-[var(--sdk-color-text-muted)]">
                <span>
                  {copy.trend.total}:{" "}
                  <strong className="text-[var(--sdk-color-text-primary)]">
                    {formatTrendValue(trendTotal, metric, locale)}
                  </strong>
                </span>
                {peak ? (
                  <span>
                    {copy.trend.maxDay}: {formatUtcDay(peak.date, locale)} (
                    {formatTrendValue(peak.value, metric, locale)})
                  </span>
                ) : null}
              </div>
            </div>
          )}
        </div>
        <div className="space-y-2">
          <h3 className="text-sm font-semibold text-[var(--sdk-color-text-primary)]">
            {copy.hostnames.title}
          </h3>
          <p className="text-xs text-[var(--sdk-color-text-muted)]">{copy.hostnames.description}</p>
          <DataTable
            className="max-h-64"
            columns={hostnameColumns}
            density="compact"
            emptyDescription={copy.hostnames.empty}
            getRowId={(row) => row.hostname}
            loading={loading}
            rows={[...byHostname]}
            stickyHeader
          />
        </div>
      </div>

      <div className="space-y-2">
        <h3 className="text-sm font-semibold text-[var(--sdk-color-text-primary)]">
          {copy.ledger.title}
        </h3>
        <p className="text-xs text-[var(--sdk-color-text-muted)]">{copy.ledger.description}</p>
        <DataTable
          className="max-h-72"
          columns={ledgerColumns}
          density="compact"
          emptyDescription={copy.ledger.empty}
          getRowId={(row) => row.id}
          loading={loading}
          rows={[...ledger]}
          stickyHeader
        />
      </div>

      <p className="text-xs leading-5 text-[var(--sdk-color-text-muted)]">{copy.billingHint}</p>
    </section>
  );
}

/* --------------------------------- storage -------------------------------- */

function StoragePanel({
  locale,
  messages,
  snapshot,
}: {
  locale: string | undefined;
  messages: ApplicationDetailMessages;
  snapshot?: ApplicationUsageSnapshot | undefined;
}) {
  const copy = messages.usage.storage;
  if (!snapshot) {
    return (
      <PanelShell icon={<Database className="h-4 w-4" />} title={copy.title}>
        <p className="text-sm text-[var(--sdk-color-text-muted)]">{messages.usage.noData}</p>
      </PanelShell>
    );
  }
  const { storage } = snapshot;
  const quotaColumns: DataTableColumn<ApplicationStorageQuota>[] = [
    {
      cell: (row) => <span className="font-mono text-xs">{row.label}</span>,
      header: copy.quotaScope,
      id: "scope",
    },
    {
      align: "right",
      cell: (row) => formatByteSize(row.usedBytes, locale),
      header: copy.quotaUsed,
      id: "used",
    },
    {
      align: "right",
      cell: (row) => formatByteSize(row.limitBytes, locale),
      header: copy.quotaLimit,
      id: "limit",
    },
    {
      align: "right",
      cell: (row) => formatByteSize(row.singleFileLimitBytes, locale),
      header: copy.quotaSingleFile,
      id: "single",
    },
  ];
  return (
    <PanelShell icon={<Database className="h-4 w-4" />} title={copy.title}>
      <p className="text-xs text-[var(--sdk-color-text-muted)]">{copy.description}</p>
      {storage.availability !== "available" ? (
        <StatusNotice tone="warning">
          {reasonText(messages.usage.unavailable, storage.availabilityReason)}
        </StatusNotice>
      ) : (
        <ul className="space-y-2" data-usage-storage>
          {storage.items.length === 0 ? (
            <li className="text-sm text-[var(--sdk-color-text-muted)]">
              {copy.scopes}: {messages.usage.noData}
            </li>
          ) : (
            storage.items.map((item) => (
              <li
                className="flex items-center justify-between gap-3 rounded-[var(--sdk-radius-control)] bg-[var(--sdk-color-surface-panel-muted)] px-3 py-2"
                key={`${item.sourceLabel}:${item.label}`}
              >
                <span
                  className="min-w-0 truncate text-xs text-[var(--sdk-color-text-secondary)]"
                  title={item.label}
                >
                  {item.label}
                </span>
                <span className="shrink-0 text-sm font-medium tabular-nums text-[var(--sdk-color-text-primary)]">
                  {formatByteSize(item.usedBytes, locale)}
                </span>
              </li>
            ))
          )}
        </ul>
      )}
      <div className="space-y-1">
        <h4 className="flex items-center gap-2 text-xs font-semibold uppercase tracking-[0.12em] text-[var(--sdk-color-text-muted)]">
          <Package className="h-3.5 w-3.5" />
          {copy.packages}
          <span className="font-normal normal-case tracking-normal">
            {templateMessage(copy.packageCount, { count: String(storage.packages.length) })}
          </span>
        </h4>
        {storage.packages.length === 0 ? (
          <p className="text-xs text-[var(--sdk-color-text-muted)]">{copy.packageEmpty}</p>
        ) : (
          <ul className="space-y-1">
            {storage.packages.slice(0, 6).map((item) => (
              <li className="flex items-center justify-between gap-3 text-xs" key={item.id}>
                <span className="flex min-w-0 items-center gap-2">
                  <span className="truncate font-mono">{item.semanticVersion}</span>
                  {item.packageFormat ? <Badge variant="outline">{item.packageFormat}</Badge> : null}
                  {item.status ? <Badge variant="secondary">{item.status}</Badge> : null}
                </span>
                <span className="shrink-0 tabular-nums text-[var(--sdk-color-text-secondary)]">
                  {formatByteSize(item.sizeBytes, locale)}
                </span>
              </li>
            ))}
          </ul>
        )}
      </div>
      <div className="space-y-2">
        <h4 className="text-xs font-semibold uppercase tracking-[0.12em] text-[var(--sdk-color-text-muted)]">
          {copy.quotas}
        </h4>
        <DataTable
          columns={quotaColumns}
          density="compact"
          emptyDescription={copy.quotaEmpty}
          getRowId={(row) => row.id}
          rows={[...storage.quotas]}
        />
      </div>
    </PanelShell>
  );
}

/* -------------------------------- readiness -------------------------------- */

function ReadinessPanel({
  delivery,
  messages,
}: {
  delivery: ApplicationDeliveryDetail;
  messages: ApplicationDetailMessages;
}) {
  const copy = messages.readiness;
  const { readiness } = delivery;
  if (readiness.availability !== "available") {
    return (
      <PanelShell icon={<ShieldCheck className="h-4 w-4" />} title={copy.title}>
        <StatusNotice tone="warning">
          {reasonText(messages.usage.unavailable, readiness.availabilityReason)}
        </StatusNotice>
      </PanelShell>
    );
  }
  const done = readiness.items.filter(
    (check) => check.status === "READY" || check.status === "NOT_APPLICABLE",
  ).length;
  const ratio = readiness.items.length === 0 ? 0 : (done / readiness.items.length) * 100;
  return (
    <PanelShell icon={<ShieldCheck className="h-4 w-4" />} title={copy.title}>
      <div className="flex flex-wrap items-center justify-between gap-2">
        <StatusBadge
          label={readinessStatusLabel(copy.status, readiness.status)}
          showIcon
          status={(readiness.status ?? "PENDING").toLowerCase()}
          variant={readinessStatusVariant(readiness.status)}
        />
        {readiness.blockingStep ? (
          <span className="text-xs text-[var(--sdk-color-state-warning)]">
            {copy.blocking}:{" "}
            {copy.steps[readiness.blockingStep as keyof typeof copy.steps] ?? readiness.blockingStep}
          </span>
        ) : null}
      </div>
      <Progress value={ratio} />
      <p className="text-xs text-[var(--sdk-color-text-muted)]">{copy.description}</p>
      {readiness.items.length === 0 ? (
        <p className="text-sm text-[var(--sdk-color-text-muted)]">{copy.empty}</p>
      ) : (
        <ol className="space-y-1.5" data-readiness-checks>
          {readiness.items.map((check) => (
            <li
              className="flex items-start justify-between gap-3 rounded-[var(--sdk-radius-control)] bg-[var(--sdk-color-surface-panel-muted)] px-3 py-2"
              data-readiness-step={check.step}
              key={check.step}
            >
              <span className="min-w-0">
                <span className="block text-sm text-[var(--sdk-color-text-primary)]">
                  {check.step === "UNKNOWN" ? check.step : copy.steps[check.step]}
                </span>
                {check.reason || check.detail ? (
                  <span className="block text-xs text-[var(--sdk-color-text-muted)]">
                    {check.reason ?? check.detail}
                  </span>
                ) : null}
              </span>
              <Badge variant={readinessStepVariant(check.status)}>
                {readinessStepLabel(copy.stepStatuses, check.status)}
              </Badge>
            </li>
          ))}
        </ol>
      )}
    </PanelShell>
  );
}

/* --------------------------------- domains -------------------------------- */

function DomainsPanel({
  copied,
  delivery,
  messages,
  onCopy,
}: {
  copied: string | null;
  delivery: ApplicationDeliveryDetail;
  messages: ApplicationDetailMessages;
  onCopy: (key: string, value: string) => void;
}) {
  const copy = messages.domains;
  const { domains } = delivery;
  if (domains.availability !== "available") {
    return (
      <PanelShell icon={<Globe2 className="h-4 w-4" />} title={copy.title}>
        <StatusNotice tone="warning">
          {reasonText(messages.usage.unavailable, domains.availabilityReason)}
        </StatusNotice>
      </PanelShell>
    );
  }
  return (
    <PanelShell icon={<Globe2 className="h-4 w-4" />} title={copy.title}>
      <p className="text-xs text-[var(--sdk-color-text-muted)]">{copy.description}</p>
      {domains.items.length === 0 ? (
        <p className="text-sm text-[var(--sdk-color-text-muted)]">{copy.empty}</p>
      ) : (
        <ul className="space-y-2" data-application-domains>
          {domains.items.map((domain) => (
            <li
              className="space-y-1 rounded-[var(--sdk-radius-control)] bg-[var(--sdk-color-surface-panel-muted)] px-3 py-2"
              data-domain-hostname={domain.hostname}
              key={`${domain.hostname}:${domain.environment}`}
            >
              <div className="flex items-center justify-between gap-2">
                <span className="flex min-w-0 items-center gap-1.5">
                  <a
                    className="truncate font-mono text-xs text-[var(--sdk-color-text-primary)] underline-offset-2 hover:underline"
                    href={`https://${domain.hostname}${domain.pathPrefix ?? ""}`}
                    rel="noreferrer"
                    target="_blank"
                  >
                    {domain.hostname}
                  </a>
                  <Button
                    aria-label={`${copy.copy} ${domain.hostname}`}
                    onClick={() => onCopy(`domain:${domain.hostname}`, domain.hostname)}
                    size="sm"
                    type="button"
                    variant="ghost"
                  >
                    <Copy className="h-3 w-3" />
                  </Button>
                </span>
                <span className="flex shrink-0 items-center gap-1">
                  {domain.isCanonical ? <Badge variant="default">{copy.canonical}</Badge> : null}
                  <Badge variant="outline">
                    {copy.kinds[domain.kind as keyof typeof copy.kinds] ?? domain.kind}
                  </Badge>
                </span>
              </div>
              <div className="flex flex-wrap items-center gap-1.5 text-[11px]">
                <Badge variant="secondary">{domain.environment}</Badge>
                <StatusBadge
                  label={domain.bindingStatus}
                  status={domain.bindingStatus.toLowerCase()}
                  variant={bindingVariant(domain.bindingStatus)}
                />
                <StatusBadge
                  label={domain.verificationStatus}
                  status={domain.verificationStatus.toLowerCase()}
                  variant={verificationVariant(domain.verificationStatus)}
                />
              </div>
              {domain.dnsRecordName || domain.dnsRecordValue ? (
                <p className="break-all font-mono text-[11px] text-[var(--sdk-color-text-muted)]">
                  {copy.dnsHint}: {domain.dnsRecordName} = {domain.dnsRecordValue}
                </p>
              ) : null}
              {domain.cnameTarget ? (
                <p className="break-all font-mono text-[11px] text-[var(--sdk-color-text-muted)]">
                  {copy.cnameHint}: {domain.cnameTarget}
                </p>
              ) : null}
              {copied === `domain:${domain.hostname}` ? (
                <p className="text-[11px] text-[var(--sdk-color-state-success)]">{copy.copied}</p>
              ) : null}
            </li>
          ))}
        </ul>
      )}
    </PanelShell>
  );
}

/* ------------------------------ delivery history --------------------------- */

function DeliveryHistorySection({
  delivery,
  messages,
}: {
  delivery: ApplicationDeliveryDetail;
  messages: ApplicationDetailMessages;
}) {
  const copy = messages.history;
  const deploymentColumns: DataTableColumn<ApplicationDeploymentRecord>[] = [
    {
      cell: (row) => (
        <span className="flex items-center gap-1.5">
          <StatusBadge
            label={row.deploymentStatus}
            status={row.deploymentStatus.toLowerCase()}
            variant={deploymentVariant(row.deploymentStatus)}
          />
          {row.rollbackFromDeploymentId ? <Badge variant="warning">{copy.rollbackBadge}</Badge> : null}
        </span>
      ),
      header: copy.columnsDeployment.status,
      id: "status",
    },
    { cell: (row) => row.environment, header: copy.columnsDeployment.environment, id: "environment" },
    {
      cell: (row) => (
        <BriefCell
          compact
          primary={row.deploymentKind ?? "—"}
          {...(row.deploymentTarget ? { secondary: row.deploymentTarget } : {})}
        />
      ),
      header: copy.columnsDeployment.kind,
      id: "kind",
    },
    {
      cell: (row) => formatDateTime(row.startedAt ?? row.createdAt),
      header: copy.columnsDeployment.startedAt,
      id: "startedAt",
    },
    {
      cell: (row) => formatDateTime(row.completedAt),
      header: copy.columnsDeployment.completedAt,
      id: "completedAt",
    },
  ];
  const releaseColumns: DataTableColumn<ApplicationReleaseRecord>[] = [
    {
      cell: (row) => (
        <span className="font-mono text-xs">
          {row.semanticVersion}
          {row.buildNumber ? `+${row.buildNumber}` : ""}
        </span>
      ),
      header: copy.columnsRelease.version,
      id: "version",
    },
    {
      cell: (row) => <Badge variant={releaseVariant(row.releaseStatus)}>{row.releaseStatus}</Badge>,
      header: copy.columnsRelease.status,
      id: "status",
    },
    {
      cell: (row) => formatDateTime(row.createdAt),
      header: copy.columnsRelease.createdAt,
      id: "createdAt",
    },
  ];
  return (
    <section
      aria-label={copy.title}
      className="space-y-3 rounded-[var(--sdk-radius-panel)] border border-[var(--sdk-color-border-default)] bg-[var(--sdk-color-surface-panel)] p-4"
      data-application-delivery-history
    >
      <div className="min-w-0">
        <h2 className="flex items-center gap-2 text-base font-semibold text-[var(--sdk-color-text-primary)]">
          <Rocket className="h-4 w-4" />
          {copy.title}
        </h2>
        <p className="text-sm text-[var(--sdk-color-text-muted)]">{copy.description}</p>
      </div>
      <div className="grid gap-4 xl:grid-cols-2">
        <div className="space-y-2">
          <h3 className="text-sm font-semibold text-[var(--sdk-color-text-primary)]">
            {copy.deployments}
          </h3>
          {delivery.deployments.availability !== "available" ? (
            <StatusNotice tone="warning">
              {reasonText(messages.usage.unavailable, delivery.deployments.availabilityReason)}
            </StatusNotice>
          ) : (
            <DataTable
              className="max-h-64"
              columns={deploymentColumns}
              density="compact"
              emptyDescription={copy.empty}
              getRowId={(row) => row.id}
              rows={[...delivery.deployments.items]}
              stickyHeader
            />
          )}
        </div>
        <div className="space-y-2">
          <h3 className="text-sm font-semibold text-[var(--sdk-color-text-primary)]">
            {copy.releases}
          </h3>
          {delivery.releases.availability !== "available" ? (
            <StatusNotice tone="warning">
              {reasonText(messages.usage.unavailable, delivery.releases.availabilityReason)}
            </StatusNotice>
          ) : (
            <DataTable
              className="max-h-64"
              columns={releaseColumns}
              density="compact"
              emptyDescription={copy.empty}
              getRowId={(row) => row.id}
              rows={[...delivery.releases.items]}
              stickyHeader
            />
          )}
        </div>
      </div>
    </section>
  );
}

/* --------------------------------- actions -------------------------------- */

/**
 * Header commands that open host-owned operations.
 *
 * The page never re-implements a write: each control calls the capability the
 * host supplied, and a command the host did not supply is simply not shown. That
 * keeps exactly one implementation of each operation in the product while still
 * letting this page be the place an operator starts from.
 */
function DetailActionGroup({
  appId,
  appKind,
  capabilities,
  canReadSource,
  messages,
}: {
  appId: string;
  appKind: string;
  capabilities?: DeploymentApplicationDetailCapabilityPort | undefined;
  canReadSource: boolean;
  messages: ApplicationDetailMessages;
}) {
  const hasConfiguration = Boolean(capabilities?.openConfiguration) && appKind !== "API_SERVICE";
  return (
    <div className="flex flex-wrap items-center gap-2" data-application-detail-actions>
      {hasConfiguration ? (
        <Button
          onClick={() => capabilities?.openConfiguration?.(appId)}
          type="button"
          variant="outline"
        >
          <Settings2 className="h-4 w-4" />
          {messages.actions.configure}
        </Button>
      ) : null}
      {capabilities?.openAccessToken ? (
        <Button onClick={() => capabilities.openAccessToken?.(appId)} type="button" variant="outline">
          <Wrench className="h-4 w-4" />
          {messages.actions.accessToken}
        </Button>
      ) : null}
      {capabilities?.openSource && canReadSource ? (
        <Button onClick={() => capabilities.openSource?.(appId)} type="button" variant="outline">
          <Clock className="h-4 w-4" />
          {messages.actions.source}
        </Button>
      ) : null}
      {capabilities?.openKindCorrection ? (
        <Button onClick={() => capabilities.openKindCorrection?.(appId)} type="button" variant="ghost">
          {messages.actions.correctKind}
        </Button>
      ) : null}
    </div>
  );
}

/* -------------------------------- primitives ------------------------------- */

function PanelShell({
  children,
  icon,
  title,
}: {
  children: ReactNode;
  icon: ReactNode;
  title: string;
}) {
  return (
    <section className="space-y-3 rounded-[var(--sdk-radius-panel)] border border-[var(--sdk-color-border-default)] bg-[var(--sdk-color-surface-panel)] p-4">
      <h2 className="flex items-center gap-2 text-sm font-semibold text-[var(--sdk-color-text-primary)]">
        {icon}
        {title}
      </h2>
      {children}
    </section>
  );
}

function BriefCell({
  compact = false,
  primary,
  secondary,
}: {
  compact?: boolean;
  primary: ReactNode;
  secondary?: string;
}) {
  return (
    <div className="min-w-0">
      <div
        className={[
          "truncate text-[var(--sdk-color-text-primary)]",
          compact ? "text-xs" : "text-sm font-medium",
        ].join(" ")}
      >
        {primary}
      </div>
      {secondary ? (
        <div className="truncate font-mono text-[11px] text-[var(--sdk-color-text-muted)]">
          {secondary}
        </div>
      ) : null}
    </div>
  );
}

/* --------------------------------- helpers -------------------------------- */

function ownershipScopeOf(detail: DeployApplicationDetail): string {
  const scope = detail.ownershipScope ?? detail.ownerType;
  if (scope === "PLATFORM" || scope === "TENANT" || scope === "ORGANIZATION" || scope === "USER") {
    return scope;
  }
  return detail.organizationId && detail.organizationId !== "0" ? "ORGANIZATION" : "TENANT";
}

function ownershipLabel(detail: DeployApplicationDetail, messages: ApplicationDetailMessages): string {
  const copy = messages.overview;
  switch (ownershipScopeOf(detail)) {
    case "ORGANIZATION":
      return copy.fields.organization;
    case "TENANT":
      return copy.fields.tenant;
    default:
      return copy.ownershipFallback;
  }
}

function appKindLabel(messages: ApplicationDetailMessages, kind: string): string {
  const kinds = messages.overview.kinds as Record<string, string>;
  return kinds[kind] ?? kinds["UNKNOWN"] ?? kind;
}

function appStatusLabel(messages: ApplicationDetailMessages, status: string): string {
  const statuses = messages.overview.statuses as Record<string, string>;
  return statuses[status] ?? statuses["UNKNOWN"] ?? status;
}

function appStatusVariant(status: string): StatusBadgeVariant {
  switch (status as DeployApplicationStatus) {
    case "ACTIVE":
      return "success";
    case "READY":
    case "PAUSED":
      return "warning";
    case "FAILED":
      return "danger";
    case "ARCHIVED":
      return "secondary";
    default:
      return "default";
  }
}

function readinessStatusLabel(
  labels: ApplicationDetailMessages["readiness"]["status"],
  status: string | undefined,
): string {
  const record = labels as Record<string, string>;
  return record[status ?? "UNKNOWN"] ?? record["UNKNOWN"] ?? status ?? "UNKNOWN";
}

function readinessStatusVariant(status: string | undefined): StatusBadgeVariant {
  switch (status) {
    case "READY":
      return "success";
    case "BLOCKED":
      return "danger";
    case "PENDING":
      return "warning";
    default:
      return "default";
  }
}

function readinessStepLabel(
  labels: ApplicationDetailMessages["readiness"]["stepStatuses"],
  status: string,
): string {
  const record = labels as Record<string, string>;
  return record[status] ?? record["UNKNOWN"] ?? status;
}

function readinessStepVariant(
  status: string,
): "success" | "warning" | "danger" | "secondary" | "outline" {
  switch (status) {
    case "READY":
      return "success";
    case "PENDING":
      return "warning";
    case "BLOCKED":
      return "danger";
    case "EXTERNAL":
      return "outline";
    default:
      return "secondary";
  }
}

function bindingVariant(status: string): StatusBadgeVariant {
  switch (status.toUpperCase()) {
    case "ACTIVE":
    case "VERIFIED":
      return "success";
    case "PENDING":
    case "PAUSED":
      return "warning";
    case "FAILED":
      return "danger";
    case "ARCHIVED":
      return "secondary";
    default:
      return "default";
  }
}

function verificationVariant(status: string): StatusBadgeVariant {
  switch (status.toUpperCase()) {
    case "NOT_REQUIRED":
    case "VERIFIED":
      return "success";
    case "PENDING":
      return "warning";
    case "FAILED":
    case "EXPIRED":
      return "danger";
    default:
      return "default";
  }
}

function deploymentVariant(status: string): StatusBadgeVariant {
  switch (status.toUpperCase()) {
    case "LIVE":
    case "ACTIVE":
    case "APPROVED":
      return "success";
    case "FAILED":
    case "ROLLED_BACK":
    case "REJECTED":
      return "danger";
    case "DEGRADED":
    case "PENDING_REVIEW":
    case "IN_REVIEW":
    case "SUBMITTING":
      return "warning";
    case "CANCELLED":
      return "secondary";
    default:
      return "default";
  }
}

function releaseVariant(
  status: string,
): "default" | "secondary" | "success" | "warning" | "danger" | "outline" {
  switch (status.toUpperCase()) {
    case "ACTIVE":
      return "success";
    case "DRAFT":
      return "warning";
    case "RETIRED":
      return "danger";
    case "ARCHIVED":
    case "SUPERSEDED":
    case "DEPRECATED":
      return "secondary";
    default:
      return "outline";
  }
}

function dimensionLabel(labels: Record<string, string>, dimension: string): string {
  return labels[dimension] ?? dimension;
}

function reasonText(
  labels: Record<ApplicationUsageAvailabilityReason, string>,
  reason: ApplicationUsageAvailabilityReason | undefined,
): string {
  return reason ? labels[reason] : labels.unavailable;
}

function shortId(value: string | undefined): string | undefined {
  if (!value) {
    return undefined;
  }
  return value.length > 12 ? `${value.slice(0, 8)}…` : value;
}
