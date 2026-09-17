import type { CloudAccountResponse } from './cloud-account-response';

export interface CloudAccountRegistrationResponse {
  account: CloudAccountResponse;
  /** True when an equivalent account already existed and was reused, so the console reports 已存在，直接复用 rather than 已创建. */
  reused: boolean;
  /** True when this call stored the secret. A reused account that already had a credential is left alone: silently rotating a credential another business module may be using is not this flow's decision to make. */
  credentialApplied: boolean;
}
