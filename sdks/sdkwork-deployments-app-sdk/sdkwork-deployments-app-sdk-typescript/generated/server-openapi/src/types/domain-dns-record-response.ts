export interface DomainDnsRecordResponse {
  id: string;
  /** zone 内记录的绝对 owner，例如 `www.example.com`。 */
  recordName: string;
  /** zone 相对主机记录（`@` 为 apex，`www`，`*`）。 */
  host: string;
  /** 记录类型，沿用服务商拼写（A、AAAA、CNAME、TXT、MX）。 */
  recordType: string;
  /** 记录值 —— A/AAAA 为解析 IP，CNAME/MX 为目标。 */
  recordValue: string;
  ttlSeconds?: number;
  /** MX/SRV 等携带优先级的记录类型的优先级。 */
  priority?: number;
  /** 服务商解析线路；按线路拆分记录时为该行所属线路。 */
  recordLine?: string;
  /** 服务商侧解析状态；DISABLED 为已暂停。 */
  recordStatus: 'ENABLED' | 'DISABLED';
  /** 该记录解析到的已登记 hostname；owner 未匹配任何 hostname 时省略。 */
  hostnameId?: string;
  /** 快照读取自的服务商家族。 */
  dnsProvider: string;
  /** 读取快照所用的云账号；部署级凭据应答时为 `deployment-config`。 */
  providerAccountId: string;
  /** 服务商返回的记录标识（如有）。 */
  providerRecordRef?: string;
  /** 该行从服务商读取的时间。 */
  syncedAt: string;
}
