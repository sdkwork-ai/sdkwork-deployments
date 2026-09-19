/**
 * Turns the canonical TXT record name into the owner a DNS provider's console
 * actually asks for.
 *
 * The API hands back the *fully qualified* name (`_sdkwork-verification.
 * birdcoder.com`) because that is the unambiguous thing to hand back. Every DNS
 * provider's console, however, asks for a **host record** relative to the zone
 * it already knows the operator selected — typing the whole name into Cloudflare
 * under a `birdcoder.com` zone publishes `_sdkwork-verification.birdcoder.com.
 * birdcoder.com`, which never resolves and leaves the operator staring at a
 * failing check with a record that looks right.
 *
 * So the dialog shows both: the absolute name (the authority, good for `dig` and
 * for providers that want a FQDN) and this relative owner (what the provider's
 * "host"/"name"/"主机记录" box wants). The fold is the same reduction the ACME
 * engine performs against a hosted zone apex in
 * `crates/sdkwork-webserver-acme-service/src/dns.rs::dns_relative_record_name`.
 *
 * The zone apex is preferred from the server (`providerZoneRef` / zone
 * `apexHostname`) because that is the zone the operator registered with the
 * provider; deriving it from the public suffix list is only a fallback, and it
 * is wrong whenever the operator runs a deeper zone (`eu.example.com`), which is
 * exactly the case the definition allows.
 */

/** Fallback zone apex permissiveness: the same limits the service enforces. */
const MAX_NAME_LENGTH = 253;
const MAX_LABEL_LENGTH = 63;

/** Lowercases, strips a trailing dot, and folds full stops the way UTS #46 does. */
function canonicalizeDnsName(raw: string): string | undefined {
  const folded = raw.trim().replace(/[\u3002\uff0e\uff61]/g, ".").toLowerCase();
  const body = folded.endsWith(".") ? folded.slice(0, -1) : folded;
  if (body.length === 0 || body.length > MAX_NAME_LENGTH || body.includes("*")) return undefined;
  const labels = body.split(".");
  if (labels.some((label) => label.length === 0 || label.length > MAX_LABEL_LENGTH)) {
    return undefined;
  }
  return body;
}

/**
 * Reduces an absolute record name against the zone that owns it.
 *
 * Returns `undefined` rather than guessing when the record is not inside the
 * zone: a prefix the operator cannot use is worse than no row at all, because it
 * fails silently at the provider.
 */
export function relativeRecordName(recordName: string, zoneApex?: string): string | undefined {
  if (zoneApex === undefined) return undefined;
  const record = canonicalizeDnsName(recordName);
  const zone = canonicalizeDnsName(zoneApex);
  if (record === undefined || zone === undefined || record === zone) return undefined;
  const suffix = `.${zone}`;
  if (!record.endsWith(suffix)) return undefined;
  const relative = record.slice(0, -suffix.length);
  // An empty prefix would mean "the zone itself", which is what the apex row is
  // for; the verification label always leaves at least one label behind.
  return relative.length === 0 ? undefined : relative;
}
