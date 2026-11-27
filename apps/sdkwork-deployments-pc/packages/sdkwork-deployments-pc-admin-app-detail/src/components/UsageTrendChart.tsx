import type { ApplicationTrafficTrendPoint } from "../types/application-usage";
import { formatByteSize, formatUsageCount } from "../services/application-usage-format";

/**
 * Minimal SVG bar chart of one traffic series.
 *
 * Two renderings of the same data are offered on purpose:
 *
 * - the SVG bars are the visual trend a reader scans for shape (a spike, a
 *   quiet period) and carry no text of their own;
 * - the readout beside it carries the exact totals, and the ledger table below
 *   carries the raw facts.
 *
 * A chart alone is not an accessible or auditable presentation of usage — an
 * operator reconciling a bill needs the numbers — so the chart is marked
 * `aria-hidden` and every figure stays reachable in text.
 */

const CHART_HEIGHT = 96;
const BAR_GAP = 2;

export type TrafficTrendMetric = "requests" | "egressBytes";

export function TrafficTrendChart({
  metric,
  points,
}: {
  metric: TrafficTrendMetric;
  points: readonly ApplicationTrafficTrendPoint[];
}) {
  if (points.length === 0) {
    return null;
  }
  const values = points.map((point) => toNumber(point[metric]));
  const max = Math.max(...values, 1);
  const width = Math.max(points.length * 8, 100);
  const barWidth = Math.max(2, width / points.length - BAR_GAP);
  return (
    <svg
      aria-hidden="true"
      className="h-24 w-full"
      data-usage-trend={metric}
      preserveAspectRatio="none"
      role="presentation"
      viewBox={`0 0 ${width} ${CHART_HEIGHT}`}
    >
      {points.map((point, index) => {
        const value = values[index] ?? 0;
        // A zero-height bar would be invisible; the minimum keeps a day with no
        // traffic distinguishable from a day that is missing from the window.
        const height = value <= 0 ? 1 : Math.max(1, (value / max) * (CHART_HEIGHT - 4));
        return (
          <rect
            fill="var(--sdk-color-brand-primary)"
            fillOpacity={value <= 0 ? 0.15 : 0.85}
            height={height}
            key={point.date}
            rx={1}
            width={barWidth}
            x={index * (barWidth + BAR_GAP)}
            y={CHART_HEIGHT - height}
          />
        );
      })}
    </svg>
  );
}

/** Peak day of a series, with the value that made it the peak. */
export function peakTrendPoint(
  points: readonly ApplicationTrafficTrendPoint[],
  metric: TrafficTrendMetric,
): { date: string; value: number } | undefined {
  let peak: { date: string; value: number } | undefined;
  for (const point of points) {
    const value = toNumber(point[metric]);
    if (!peak || value > peak.value) {
      peak = { date: point.date, value };
    }
  }
  return peak;
}

/** Series total across every day of the window. */
export function sumTrend(
  points: readonly ApplicationTrafficTrendPoint[],
  metric: TrafficTrendMetric,
): number {
  return points.reduce((total, point) => total + toNumber(point[metric]), 0);
}

/** Series value rendered in the unit the metric is measured in. */
export function formatTrendValue(
  value: number | string | undefined,
  metric: TrafficTrendMetric,
  locale?: string,
): string {
  const text = typeof value === "number" ? String(value) : value ?? "0";
  return metric === "egressBytes"
    ? formatByteSize(text, locale)
    : formatUsageCount(text, locale);
}

function toNumber(value: string): number {
  const parsed = Number(value);
  return Number.isFinite(parsed) ? parsed : 0;
}
