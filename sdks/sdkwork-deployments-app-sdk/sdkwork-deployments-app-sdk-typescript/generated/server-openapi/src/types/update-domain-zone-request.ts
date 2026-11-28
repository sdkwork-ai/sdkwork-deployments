import type { DomainZoneFiling } from './domain-zone-filing';
import type { DomainZonePoliceFiling } from './domain-zone-police-filing';

export interface UpdateDomainZoneRequest {
  displayName?: string;
  dnsProvider?: string;
  providerZoneRef?: string;
  /** Re-pins the zone's cloud account. An empty string clears the pin so the zone resolves an account per issuance; omitting the field leaves the current pin untouched. */
  providerAccountId?: string;
  status?: 'ACTIVE' | 'PAUSED';
  /** Whole-object replacement of the root-domain ICP filing. Omit the property to leave it unchanged, send an object to replace all members, or send null to clear the filing. */
  filing?: DomainZoneFiling | null;
  /** Whole-object replacement of the root-domain public security (police) filing. Omit the property to leave it unchanged, send an object to replace all members, or send null to clear the filing. An object whose normalized members are all absent also clears the filing. */
  policeFiling?: DomainZonePoliceFiling | null;
}
