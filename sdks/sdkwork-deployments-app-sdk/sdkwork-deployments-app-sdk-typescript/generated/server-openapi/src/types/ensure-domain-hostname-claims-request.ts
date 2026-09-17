export interface EnsureDomainHostnameClaimsRequest {
  /** Fully-qualified hostnames the caller intends to cover; a leading "*." is accepted on the leftmost label only. Names outside the selected zone, and names that repeat, are rejected before any hostname row is written so a failed request never leaves the tenant owning names nobody asked to keep.
 */
  hostnames: string[];
}
