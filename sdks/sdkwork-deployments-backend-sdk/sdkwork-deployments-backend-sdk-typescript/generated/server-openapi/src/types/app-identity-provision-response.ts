export interface AppIdentityProvisionResponse {
  /** Opaque, stable public address of the application (`deploy_app.uuid`), not the decimal id IAM anchors on. */
  appId: string;
  /** The application's **decimal** primary key (`deploy_app.id`), and the **IAM installation anchor**. The same value `apps.create`, `apps.retrieve`, and `apps.update` publish as `deployApplicationId`; a re-registered installation therefore answers with the anchor it was just provisioned under instead of making the caller derive it from the uuid. */
  deployApplicationId: string;
  /** Persisted `deploy_app.identity_status` after the attempt. `failed` means the attempt ran and IAM rejected it (or was unreachable); `pending_identity` means no provisioning adapter is configured.
 */
  identityStatus: 'pending_identity' | 'active' | 'failed';
}
