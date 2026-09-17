import type { CloudAccountRegistrationResponse } from './cloud-account-registration-response';

export interface CloudAccountsCreateResponse201 {
  code: 0;
  data: unknown & { item: CloudAccountRegistrationResponse; };
  /** Server-owned request correlation id. */
  traceId: string;
}
