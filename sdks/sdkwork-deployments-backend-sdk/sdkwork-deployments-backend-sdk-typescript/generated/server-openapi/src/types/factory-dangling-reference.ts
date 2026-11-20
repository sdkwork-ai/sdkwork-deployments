export interface FactoryDanglingReference {
  authority: 'iam' | 'webserver' | 'self';
  deployApplicationId?: string;
  appId?: string;
  /** Closed finding code, stable enough for alerts. */
  code: string;
  detail: string;
}
