import type { AppSourceBindingStatus } from './app-source-binding-status';

export interface AppSourceBindingResponse {
  providerType: 'DRIVE' | 'KNOWLEDGEBASE';
  providerResourceUuid: string;
  contractVersion: string;
  status: AppSourceBindingStatus;
  updatedAt?: string;
}
