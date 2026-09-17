export interface UpdateDomainZoneRequest {
  displayName?: string;
  dnsProvider?: string;
  providerZoneRef?: string;
  /** Re-pins the zone's cloud account. An empty string clears the pin so the zone resolves an account per issuance; omitting the field leaves the current pin untouched. */
  providerAccountId?: string;
  status?: 'ACTIVE' | 'PAUSED';
}
