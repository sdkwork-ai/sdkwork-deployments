export interface DomainVerifyResponse {
  verified: boolean;
  method: 'DNS_TXT';
  /** 当前所有权验证尝试标识；域名已验证时可省略。 */
  verificationId?: string;
  /** 需要配置 TXT 记录的完整规范名称；域名已验证时可省略。 */
  recordName?: string;
  /** 同一记录相对于其所属 zone 的主机记录名（即 DNS 服务商控制台的「主机记录」/「Host」字段值）； 记录不在该 zone 内或 zone 未知时可省略。服务商控制台会自动追加自己的 zone，因此把完整名称填入该字段会发布错误的双域名记录。 */
  recordRelativeName?: string;
  /** 仅在创建验证尝试时返回一次的明文 proof；后续查询和验证响应不会再次返回。 遵循 RFC 8555 §8.4 的通用形态 base64url(sha256(secret))，为 43 个字符、无填充、不含品牌前缀。 */
  token?: string;
  /** 当前验证尝试的失效时间；域名已验证时可省略。 */
  expiresAt?: string;
}
