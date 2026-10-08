export interface DomainDnsSyncResponse {
  recordCount: string;
  syncedAt: string;
  /** 本次读取的服务商 inventory 覆盖的 zone apex。 */
  zoneApex: string;
  dnsProvider: string;
  /** 应答本次读取的云账号。 */
  providerAccountId: string;
}
