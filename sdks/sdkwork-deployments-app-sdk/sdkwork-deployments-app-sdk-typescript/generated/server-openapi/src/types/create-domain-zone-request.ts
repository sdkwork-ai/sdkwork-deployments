export interface CreateDomainZoneRequest {
  /** A registrable root domain (zone apex) such as example.com or example.co.uk. Wildcard names and subdomains are not accepted; Unicode (IDN) names are converted to punycode automatically. The apex must not already exist as a hostname and must not overlap an existing root domain.
 */
  apexHostname: string;
  displayName?: string;
  dnsProvider?: string;
  providerZoneRef?: string;
  /** Cloud account that presents this zone's DNS-01 challenges, from the platform provider account center. Optional: omitting it leaves the zone unpinned, and issuance then resolves an account by provider and falls back to the deployment-level provider configuration. An account holding no credential is refused here rather than at the first certificate order that needs it. */
  providerAccountId?: string;
}
