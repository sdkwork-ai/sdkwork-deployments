import type { AppIdentityProvisionResponse } from './app-identity-provision-response';

export interface AppsIdentityProvisionResponse {
  code: 0;
  data: unknown & { item: AppIdentityProvisionResponse; };
  /** Server-owned request correlation id. */
  traceId: string;
}
