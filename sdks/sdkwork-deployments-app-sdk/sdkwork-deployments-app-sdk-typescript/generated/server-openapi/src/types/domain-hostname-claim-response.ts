import type { DomainHostnameResponse } from './domain-hostname-response';

export interface DomainHostnameClaimResponse {
  hostname: DomainHostnameResponse;
  /** What this call's ownership check observed. The caller can never assert success (ADR-20260723 §3), so `false` means the TXT record is not published yet — it does not mean the request failed. A hostname that is already VERIFIED reports `true` without a lookup.
 */
  verified: boolean;
  /** Canonical TXT record name to publish; omitted once the hostname is verified. */
  dnsRecordName?: string;
  /** 同一记录相对于其所属 zone 的主机记录名（DNS 服务商控制台的「主机记录」/「Host」 字段值）；记录不在已知 zone 内时省略。服务商控制台会自动追加自己的 zone。 */
  dnsRecordRelativeName?: string;
  /** Record type to publish; omitted once the hostname is verified. */
  dnsRecordType?: 'TXT';
  /** Public proof digest to publish. Returned only by the call that created the verification attempt, matching the domain-verification "shown once" rule; a later check reports state without replaying the value. */
  dnsRecordValue?: string;
  /** Expiry of the outstanding attempt; omitted once the hostname is verified. */
  expiresAt?: string;
}
