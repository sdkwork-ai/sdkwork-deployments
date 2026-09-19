export interface AppDomainResponse {
  /** Fully-qualified hostname the app answers on. */
  hostname: string;
  /** `DEFAULT` for platform-provisioned `<appDomainLabel>.app[-<env>].<suffix>` hostnames, `CUSTOM` for user-registered hostnames. */
  kind: 'DEFAULT' | 'CUSTOM';
  /** Lifecycle environment the hostname serves. */
  environment: string;
  /** `deploy_app_binding.status`: PENDING / VERIFIED / ACTIVE / PAUSED / FAILED / ARCHIVED. Distinct from the DNS ownership state. */
  bindingStatus: string;
  /** DNS ownership verification of the hostname itself. `NOT_REQUIRED` for default hostnames — the platform owns the apex. */
  verificationStatus: 'NOT_REQUIRED' | 'PENDING' | 'VERIFIED' | 'FAILED' | 'EXPIRED';
  /** Whether this is the app's canonical hostname in its environment. */
  isCanonical?: boolean;
  pathPrefix?: string;
  /** Present for CUSTOM hostnames: the `deploy_domain` row id. */
  domainId?: string;
  /** Present while a CUSTOM hostname is unverified: the TXT record name the operator must publish to prove ownership. */
  dnsRecordName?: string;
  /** Present while a CUSTOM hostname is unverified: the TXT record value to publish. */
  dnsRecordValue?: string;
  /** Present for CUSTOM hostnames: the platform hostname the operator points the domain at. */
  cnameTarget?: string;
}
