export interface DomainZoneResponse {
  id: string;
  apexHostname: string;
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
