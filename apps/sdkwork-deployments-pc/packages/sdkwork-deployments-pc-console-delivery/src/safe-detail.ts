/**
 * A provider's refusal, made safe to put on screen.
 *
 * `lastErrorDetail` is the one field on a certificate that contains text we did
 * not author: it is a DNS provider's or a CA's own sentence, relayed through our
 * API. Being server-authored is not the same as being safe — it is attacker-
 * influenced text that happens to have travelled over our transport — so it goes
 * through the same guard as any other such text rather than being trusted.
 *
 * Two things this does that matter more than the sanitising:
 *
 * - **It bounds the length.** A 512-character provider sentence dropped into a
 *   table cell wrecks the layout, which is what made "just show the message" the
 *   wrong answer the first time this problem was met.
 * - **It returns `undefined` rather than an empty string** when it withholds
 *   something, so a caller's `&&` render decides nothing rather than deciding to
 *   render a blank line under a code.
 */
export function safeErrorDetail(value: unknown, maxLength = 320): string | undefined {
  if (typeof value !== "string") return undefined;
  const normalized = value.replace(/\s+/g, " ").trim();
  if (!normalized || containsSensitiveDiagnostic(normalized)) return undefined;
  return normalized.length <= maxLength
    ? normalized
    : `${normalized.slice(0, maxLength - 3).trimEnd()}...`;
}

/**
 * Diagnostics that must never be rendered: credentials the redactor on the way in
 * may not have recognised, SQL and driver text, stack frames and absolute paths.
 *
 * Deliberately conservative and value-based rather than shape-based. The text
 * arrives from an arbitrary vendor, so there is no schema to validate against and
 * no list of fields to exclude — the only thing that can be recognised is the
 * shape of the secret itself.
 */
function containsSensitiveDiagnostic(value: string): boolean {
  return [
    /-----BEGIN [A-Z ]*PRIVATE KEY-----/i,
    /\b(?:authorization|access[-_ ]?token|refresh[-_ ]?token|password|private[-_ ]?key|api[-_ ]?key|secret)\s*[:=]\s*["']?\S+/i,
    /\b(?:bearer\s+[A-Za-z0-9._~-]+|eyJ[A-Za-z0-9_-]{16,}\.[A-Za-z0-9_-]{16,})/i,
    /\b(?:select\s+.+\s+from|insert\s+into|update\s+.+\s+set|delete\s+from)\b/i,
    /\b(?:sqlx|postgres(?:ql)?|mysql|sqlite|ora-\d+)\b/i,
    /\b(?:stack trace|traceback)\b|\bat\s+\S+\s*\([^)]*:\d+:\d+\)/i,
    /(?:[A-Za-z]:\\|\/(?:home|etc|usr|var)\/)[^\s]+/i,
    /<\/?[A-Za-z][^>]*>/,
    /\b[A-Za-z0-9_-]{96,}\b/,
  ].some((pattern) => pattern.test(value));
}
