export interface DomainZoneResponse {
  id: string;
  apexHostname: string;
  /** Who owns the zone. `USER` is a root domain an operator defined and owns; `PLATFORM` is the tenant-level `app.<suffix>` zone the deployment provisions for app publishing hostnames, which has no owner and is visible to every tenant member. A client that lists "my root domains" must not present a `PLATFORM` zone as one. */
  scope: 'USER' | 'PLATFORM';
  displayName?: string;
  dnsProvider?: string;
  /** The cloud account pinned to serve this zone's DNS-01 challenges, absent when the zone has none and resolves one per issuance. */
  providerAccountId?: string;
  status: 'ACTIVE' | 'PAUSED';
  hostnameCount: string;
  verifiedHostnameCount: string;
  certificateCount: string;
  bindingCount: string;
  updatedAt: string;
  version: string;
}
