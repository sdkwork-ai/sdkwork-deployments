/**
 * Public Suffix List lookup for the browser console.
 *
 * The rule data is generated from the same `psl` crate release the service
 * locks (`tools/generate-public-suffix.mjs`), so a client-side "is this a
 * registrable root domain?" answer matches
 * `normalize_zone_apex` in `crates/sdkwork-intelligence-deploy-service/src/domain_verification.rs`.
 *
 * The matching algorithm is the reference one from
 * <https://publicsuffix.org/list/>:
 *
 *   - exception rules (`!www.ck`) win outright and mean "the public suffix is
 *     this rule minus its leftmost label";
 *   - otherwise the longest matching rule wins, where a wildcard rule matches
 *     one arbitrary label in place of the `*`;
 *   - when no rule matches at all the prevailing rule is `*`, i.e. the public
 *     suffix is the final label.
 *
 * The third clause is deliberate: the service's `psl::domain_str` returns `None`
 * for a TLD the list does not carry, but this client is a *pre-check*. It must
 * never refuse something the service would accept, so an unknown TLD falls back
 * to the reference behaviour and the service stays the authority.
 */

import { PUBLIC_SUFFIX_ASCII_MIRRORS, PUBLIC_SUFFIX_RULES } from "./public-suffix-data.ts";

let cachedRules: Set<string> | undefined;

/** Rule set, with both the upstream Unicode form and the punycode mirror. */
export function publicSuffixRules(): Set<string> {
  if (cachedRules === undefined) {
    cachedRules = new Set<string>();
    for (const block of [PUBLIC_SUFFIX_RULES, PUBLIC_SUFFIX_ASCII_MIRRORS]) {
      for (const rule of block.split("\n")) {
        if (rule.length > 0) cachedRules.add(rule);
      }
    }
  }
  return cachedRules;
}

/**
 * Number of trailing labels that form the public suffix of `labels`.
 *
 * `labels` is written left to right, so `example.com` is `["example", "com"]`.
 */
export function publicSuffixLabelCount(labels: readonly string[]): number {
  const rules = publicSuffixRules();
  const count = labels.length;
  if (count === 0) return 0;

  // Exception rules are the most specific form and are checked first.
  for (let length = count; length >= 2; length -= 1) {
    if (rules.has(`!${labels.slice(count - length).join(".")}`)) return length - 1;
  }

  let longest = 0;
  for (let length = 1; length <= count; length += 1) {
    const candidate = labels.slice(count - length).join(".");
    if (rules.has(candidate)) {
      longest = length;
      continue;
    }
    // A `*.` rule consumes one arbitrary label in front of its own labels.
    if (length >= 2 && rules.has(`*.${labels.slice(count - length + 1).join(".")}`)) {
      longest = length;
    }
  }

  return longest === 0 ? 1 : longest;
}

/**
 * Registrable domain (public suffix plus one label) of an ASCII host, or
 * `undefined` when there is no label in front of the public suffix.
 */
export function registrableDomain(asciiHost: string): string | undefined {
  const labels = asciiHost.split(".");
  if (labels.some((label) => label.length === 0)) return undefined;
  const suffix = publicSuffixLabelCount(labels);
  if (suffix === 0 || labels.length < suffix + 1) return undefined;
  return labels.slice(labels.length - suffix - 1).join(".");
}

/** True when `asciiHost` is itself a registrable root domain. */
export function isRegistrableRootDomain(asciiHost: string): boolean {
  return registrableDomain(asciiHost) === asciiHost;
}
