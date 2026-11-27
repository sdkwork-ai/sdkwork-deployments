/**
 * Shared date, value, and usage formatting of the detail surface.
 *
 * A cloud console prints quantities in the units an operator reasons in — bytes
 * as `KiB`/`MiB`/`GiB`, counts with thousands separators, percentages with one
 * decimal — and it never prints a price: pricing is Commerce's authority, and
 * the deployment plane only emits metered facts. These helpers are pure and
 * shared by the KPI cards, the trend readout, the ledger, and the storage
 * panels so one quantity always reads the same way on the page.
 */

const BYTE_UNITS = ["B", "KiB", "MiB", "GiB", "TiB", "PiB"] as const;

/** Locale-aware date + time, or the em-dash placeholder when absent/invalid. */
export function formatDateTime(value: string | undefined | null, locale?: string): string {
  if (!value) {
    return "—";
  }
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) {
    return value;
  }
  return new Intl.DateTimeFormat(locale, { dateStyle: "medium", timeStyle: "short" }).format(date);
}

/** Locale-aware date only, or the em-dash placeholder when absent/invalid. */
export function formatDate(value: string | undefined | null, locale?: string): string {
  if (!value) {
    return "—";
  }
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) {
    return value;
  }
  return new Intl.DateTimeFormat(locale, { dateStyle: "medium" }).format(date);
}

/** Locale-aware integer formatting of an exact decimal string. */
export function formatUsageCount(value: string | undefined, locale?: string): string {
  if (value === undefined || value.trim() === "") {
    return "—";
  }
  const parsed = Number(value);
  if (!Number.isFinite(parsed)) {
    return value;
  }
  return new Intl.NumberFormat(locale, { maximumFractionDigits: 0 }).format(parsed);
}

/**
 * Human-readable binary byte size.
 *
 * Chooses the largest unit that keeps the value at or above one and prints at
 * most one decimal, so a dashboard row stays scannable. A sub-kilobyte value
 * stays in bytes ("512 B"), never "0 KiB".
 */
export function formatByteSize(value: string | undefined, locale?: string): string {
  if (value === undefined || value.trim() === "") {
    return "—";
  }
  const parsed = Number(value);
  if (!Number.isFinite(parsed) || parsed < 0) {
    return value;
  }
  let unitIndex = 0;
  let scaled = parsed;
  while (scaled >= 1024 && unitIndex < BYTE_UNITS.length - 1) {
    scaled /= 1024;
    unitIndex += 1;
  }
  if (unitIndex === 0) {
    return `${formatUsageCount(String(Math.round(scaled)), locale)} B`;
  }
  const formatted = new Intl.NumberFormat(locale, {
    maximumFractionDigits: scaled >= 100 ? 0 : 1,
    minimumFractionDigits: 0,
  }).format(scaled);
  return `${formatted} ${BYTE_UNITS[unitIndex]}`;
}

/** Share of `total` that `part` represents, clamped to `0..100`. */
export function usageRatio(part: string | undefined, total: string | undefined): number | undefined {
  if (part === undefined || total === undefined) {
    return undefined;
  }
  const partValue = Number(part);
  const totalValue = Number(total);
  if (!Number.isFinite(partValue) || !Number.isFinite(totalValue) || totalValue <= 0) {
    return undefined;
  }
  return Math.min(100, Math.max(0, (partValue / totalValue) * 100));
}

/** `"12.3%"`, or the em dash when the ratio is unknown. */
export function formatRatio(ratio: number | undefined, locale?: string): string {
  if (ratio === undefined) {
    return "—";
  }
  const formatted = new Intl.NumberFormat(locale, {
    maximumFractionDigits: ratio >= 10 ? 0 : 1,
    minimumFractionDigits: 0,
  }).format(ratio);
  return `${formatted}%`;
}

/** UTC calendar day (`2026-11-27`) rendered in the operator's locale. */
export function formatUtcDay(day: string | undefined, locale?: string): string {
  if (!day) {
    return "—";
  }
  const parsed = new Date(`${day}T00:00:00.000Z`);
  if (Number.isNaN(parsed.getTime())) {
    return day;
  }
  return new Intl.DateTimeFormat(locale, {
    day: "2-digit",
    month: "2-digit",
    timeZone: "UTC",
  }).format(parsed);
}

/** The half-open day window as one readable range. */
export function formatUsageWindowRange(
  start: string | undefined,
  end: string | undefined,
  locale?: string,
): string {
  if (!start || !end) {
    return "—";
  }
  const exclusiveDay = new Date(`${end}T00:00:00.000Z`);
  if (Number.isNaN(exclusiveDay.getTime())) {
    return `${start} → ${end}`;
  }
  // The window is half-open, so the last covered day is `end - 1 day`.
  const lastDay = new Date(exclusiveDay.getTime() - 86_400_000).toISOString().slice(0, 10);
  const formatter = new Intl.DateTimeFormat(locale, {
    day: "2-digit",
    month: "short",
    timeZone: "UTC",
    year: "numeric",
  });
  const startText = formatter.format(new Date(`${start}T00:00:00.000Z`));
  const endText = formatter.format(new Date(`${lastDay}T00:00:00.000Z`));
  return startText === endText ? startText : `${startText} – ${endText}`;
}

/**
 * A measure rendered in the unit the provider declared.
 *
 * `BYTE` is formatted as a size, every other declared unit is printed next to
 * the count; an empty unit prints the count alone rather than an invented one.
 */
export function formatUsageMeasure(
  quantity: string | undefined,
  unit: string | undefined,
  locale?: string,
): string {
  if (unit === "BYTE") {
    return formatByteSize(quantity, locale);
  }
  const count = formatUsageCount(quantity, locale);
  return unit ? `${count} ${unit}` : count;
}

/** Dense row label of one usage dimension: the wire name, or `unknown`. */
export function formatUsageDimensionLabel(dimension: string | undefined): string {
  return dimension && dimension.trim() !== "" ? dimension : "unknown";
}

/** The operator's locale, read once per render pass. */
export function resolveOperatorLocale(): string | undefined {
  return typeof navigator === "undefined" ? undefined : navigator.language;
}
