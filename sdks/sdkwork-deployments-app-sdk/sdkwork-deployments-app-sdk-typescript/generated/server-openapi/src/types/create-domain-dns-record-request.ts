export interface CreateDomainDnsRecordRequest {
  recordType: 'A' | 'AAAA' | 'CNAME' | 'TXT' | 'MX' | 'NS' | 'CAA';
  /** zone 相对主机记录（`@` 为 apex，`www`，`api.eu`，`*`）。 */
  host: string;
  recordValue: string;
  ttlSeconds?: number;
  /** MX 优先级；不携带优先级的类型会被拒绝。 */
  priority?: number;
  /** 服务商解析线路；省略即默认线路。 */
  recordLine?: string;
}
