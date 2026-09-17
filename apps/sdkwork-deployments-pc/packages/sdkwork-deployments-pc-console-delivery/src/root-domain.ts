/**
 * Client-side validation for the "define root domain" dialog.
 *
 * This is a *pre-check*, not a second authority. It mirrors the service rule by
 * rule so the console can answer immediately and in the operator's language,
 * while `normalize_zone_apex` in `crates/sdkwork-intelligence-deploy-service/
 * src/domain_verification.rs` stays the thing that decides.
 *
 * | Service rule                                        | Here              |
 * |-----------------------------------------------------|-------------------|
 * | `value.trim()`                                      | `required`/trim   |
 * | one trailing `.` stripped                           | same              |
 * | `*.` prefix and any other `*` rejected by the apex  | `wildcard`        |
 * | `idna::domain_to_ascii_strict` (UTS #46)            | WHATWG `URL` host |
 * | `IpAddr::from_str`                                  | `ip`              |
 * | `len > 231`                                         | `tooLong`         |
 * | label empty or `len > 63`                           | `tooLong`         |
 * | `!contains('.')`                                    | `singleLabel`     |
 * | `psl::domain_str(host) == host`                     | `notApex`         |
 *
 * Deliberate difference: the service's `psl::domain_str` returns `None` for a
 * top-level domain the list does not carry, which makes it reject e.g.
 * `example.internal`; the reference list algorithm falls back to the `*` rule
 * and accepts it. A pre-check must never refuse what the service would accept,
 * so the reference behaviour is used and the service keeps the final word.
 */

import { isRegistrableRootDomain } from "./public-suffix.ts";

/** Why a candidate root domain cannot be used. */
export type RootDomainIssue =
  | "required"
  | "wildcard"
  | "malformed"
  | "ip"
  | "singleLabel"
  | "tooLong"
  | "notApex";

export type RootDomainValidation =
  | {
      ok: true;
      /** Canonical value for the wire: lowercase ASCII, no trailing dot. */
      value: string;
      /** True when the input had to be converted to its punycode form. */
      converted: boolean;
    }
  | { ok: false; issue: RootDomainIssue };

/** Longest accepted host, mirroring the service's `ascii.len() > 231` guard. */
const MAX_LENGTH = 231;
const MAX_LABEL_LENGTH = 63;

/** Dotted-quad literal, the only IP form that survives host validation. */
function isIpv4(value: string): boolean {
  const parts = value.split(".");
  return (
    parts.length === 4 &&
    parts.every(
      (part) => /^\d{1,3}$/.test(part) && part.length === String(Number(part)).length && Number(part) <= 255,
    )
  );
}

/**
 * Validate one root-domain candidate.
 *
 * The answer is independent of locale and of the dialog: it only encodes what
 * the service will accept.
 */
export function validateRootDomain(raw: string): RootDomainValidation {
  const trimmed = raw.trim();
  if (trimmed.length === 0) return { ok: false, issue: "required" };
  if (trimmed.includes("*")) {
    return { ok: false, issue: trimmed.startsWith("*.") ? "wildcard" : "malformed" };
  }

  // The service strips exactly one trailing dot and then rejects any remaining
  // empty label, so `example.com.` is valid and `example.com..` is not.
  //
  // UTS #46 (and therefore `idna::domain_to_ascii_strict`) folds the other
  // full-stop code points onto `.`, and a Chinese IME produces those readily,
  // so they are folded here too rather than being rejected as stray characters.
  const normalised = trimmed.replace(/[\u3002\uff0e\uff61]/g, ".");
  const body = normalised.endsWith(".") ? normalised.slice(0, -1) : normalised;
  if (body.length === 0) return { ok: false, issue: "malformed" };

  // Anything outside letters, digits, dots, and hyphens is a pasted URL, a
  // port, a path, an underscore, or whitespace — reject before the URL parser
  // can helpfully split a path off instead of reporting it.
  if (/[^\p{L}\p{N}.-]/u.test(body)) return { ok: false, issue: "malformed" };

  let ascii: string;
  try {
    ascii = new URL(`http://${body}`).hostname;
  } catch {
    return { ok: false, issue: "malformed" };
  }
  if (!/^[a-z0-9.-]+$/.test(ascii)) return { ok: false, issue: "malformed" };
  if (isIpv4(ascii)) return { ok: false, issue: "ip" };

  const labels = ascii.split(".");
  if (ascii.length > MAX_LENGTH || labels.some((label) => label.length > MAX_LABEL_LENGTH)) {
    return { ok: false, issue: "tooLong" };
  }
  if (labels.some((label) => label.length === 0 || label.startsWith("-") || label.endsWith("-"))) {
    return { ok: false, issue: "malformed" };
  }
  if (labels.length < 2) return { ok: false, issue: "singleLabel" };
  if (!isRegistrableRootDomain(ascii)) return { ok: false, issue: "notApex" };

  return { ok: true, value: ascii, converted: ascii !== body.toLowerCase() };
}
