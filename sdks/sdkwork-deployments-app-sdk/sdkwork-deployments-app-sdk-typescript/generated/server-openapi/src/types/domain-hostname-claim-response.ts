import type { DomainHostnameResponse } from './domain-hostname-response';

export interface DomainHostnameClaimResponse {
  hostname: DomainHostnameResponse;
  /** What this call's ownership check observed. The caller can never assert success (ADR-20260723 §3), so `false` means the TXT record is not published yet — it does not mean the request failed. A hostname that is already VERIFIED reports `true` without a lookup.
 */
  verified: boolean;
  /** Canonical TXT record name to publish; omitted once the hostname is verified. */
  dnsRecordName?: string;
  /** Record type to publish; omitted once the hostname is verified. */
  dnsRecordType?: 'TXT';
  /** Public proof digest to publish. Returned only by the call that created the verification attempt, matching the domain-verification "shown once" rule; a later check reports state without replaying the value. */
  dnsRecordValue?: string;
  /** Expiry of the outstanding attempt; omitted once the hostname is verified. */
  expiresAt?: string;
}
