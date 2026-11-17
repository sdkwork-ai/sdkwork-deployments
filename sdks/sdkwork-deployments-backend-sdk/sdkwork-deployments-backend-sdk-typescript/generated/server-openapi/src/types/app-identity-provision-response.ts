export interface AppIdentityProvisionResponse {
  appId: string;
  /** Persisted `deploy_app.identity_status` after the attempt. `failed` means the attempt ran and IAM rejected it (or was unreachable); `pending_identity` means no provisioning adapter is configured.
 */
  identityStatus: 'pending_identity' | 'active' | 'failed';
}
