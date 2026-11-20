export interface AppUninstallResponse {
  appId: string;
  uninstallStatus: 'not_started' | 'pending_identity_disable' | 'identity_disabled' | 'complete' | 'failed';
  appStatus: 'DRAFT' | 'ACTIVE' | 'PAUSED' | 'ARCHIVED';
  replayed: boolean;
}
