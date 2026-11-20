export interface FactoryReadinessCheck {
  step: 'REGISTRATION' | 'PLATFORM_TARGET' | 'ENVIRONMENT' | 'IDENTITY' | 'SOURCE' | 'HOSTING_CARRIER';
  status: 'READY' | 'PENDING' | 'BLOCKED' | 'EXTERNAL' | 'NOT_APPLICABLE';
  reason?: string;
  detail?: string;
}
