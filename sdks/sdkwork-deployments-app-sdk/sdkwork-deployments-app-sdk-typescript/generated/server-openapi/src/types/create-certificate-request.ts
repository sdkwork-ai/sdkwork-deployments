/** Identifier set 由 `domainIds` 或 `requestedIdentifiers` 二者之一给出，必须恰好提供其中一个。 两者同时出现是校验错误（`CERTIFICATE_IDENTIFIER_SOURCE_CONFLICT`）， 而不是取并集或交集：任何一种隐式合并都等于改写操作者的请求。 */
export interface CreateCertificateRequest {
  certName: string;
  /** 以域名声明（`deploy_domain`）的 id 指定证书要覆盖的主机名，即控制台选择器选中的已验证主机名。 每个 id 都必须解析为本租户中 ACTIVE 且 VERIFIED、且其所属 DNS 区域对本调用者可见的声明； 命名一个 id 从不绕过该所有权校验。与 `requestedIdentifiers` 互斥。 */
  domainIds?: string[];
  /** 直接以 FQDN 指定证书要覆盖的主机名，即操作者在自由输入框中键入的域名，而不是预先选中的声明 id。 每个提交的主机名先按契约自身的规范化规则处理（小写、去掉尾部点、只允许单个前导通配符标签）。 `MANAGED` 模式下，每个主机名必须解析为本租户中 ACTIVE 且 VERIFIED、且其所属 DNS 区域对本调用者可见的声明； `MANUAL` + `DNS_01` 模式下，DNS-01 TXT 验证本身就是所有权证明，因此允许尚未预验证的主机名。 手动模式会为缺失的主机名创建 PENDING 声明；若同名主机名或所属命名空间已被其他租户或本租户的私有区域占用，请求仍被拒绝。 通配符范围追加的 apex 使用同一规则。与 `domainIds` 互斥。 */
  requestedIdentifiers?: string[];
  /** `SINGLE_DOMAIN` issues for the exact names supplied. `WILDCARD` additionally plans the apex of each wildcard, because a wildcard SAN does not cover its own apex, and forces DNS-01 validation. */
  certificateScope?: 'SINGLE_DOMAIN' | 'WILDCARD';
  /** Operator preference. `AUTO` resolves to HTTP_01 for a managed `SINGLE_DOMAIN` request and to DNS_01 for `WILDCARD` or `MANUAL`. `HTTP_01` is rejected for a `WILDCARD` or `MANUAL` request. */
  validationMethod?: 'AUTO' | 'HTTP_01' | 'DNS_01';
  /** 由谁完成挑战应答。`MANAGED`（默认）由平台通过已配置的 DNS 供应商凭据 自动发布并撤回记录；`MANUAL` 由 API 返回精确的 TXT 记录，等待操作者在 自己的 DNS 中发布后调用 `certificates.challenge.verify` 放行校验。 与 `validationMethod` 正交：DNS_01 在两种模式下都合法，而 `MANUAL` 与 `HTTP_01` 组合会被规划规则拒绝（边缘 webroot 不可由操作者写入）。 */
  challengePresentation?: 'MANAGED' | 'MANUAL';
  caProfile?: 'LETS_ENCRYPT_STAGING' | 'LETS_ENCRYPT_PRODUCTION';
  /** Key algorithm of the issued leaf. Defaults to RSA: a managed certificate is renewed unattended and RSA-2048 is the leaf key every TLS client accepts, so a certificate requested without an opinion on this stays reachable from old stacks as well. Ask for ECDSA explicitly where every client is known to support P-256. */
  preferredKeyAlgorithm?: 'RSA' | 'ECDSA';
  /** Whether the control plane keeps this certificate renewed on its own. Defaults to true: asking for a managed certificate means asking for a name that stays covered, not for a reminder to renew it. */
  autoRenew?: boolean;
  /** Days before expiry at which renewal starts. Stored per certificate rather than read from the app binding's TLS policy, because one certificate may back several listeners with different policies and a renewal trigger has to resolve to exactly one window. The configured lead time is additionally floored at one third of the certificate's lifetime, so a value larger than a short-lived certificate's lifetime cannot make a fresh certificate immediately due. */
  renewBeforeDays?: number;
  /** Cloud account that presents this certificate's DNS-01 challenges, overriding the zone's pin so one zone can issue through several vendor accounts. Optional: omitting it leaves the certificate deferring to its zone, then to the account center, then to the deployment-level provider configuration, decided when an order runs. */
  providerAccountId?: string;
}
