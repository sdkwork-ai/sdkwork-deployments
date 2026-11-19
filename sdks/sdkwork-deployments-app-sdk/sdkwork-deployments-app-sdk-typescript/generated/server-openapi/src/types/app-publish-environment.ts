/** The five lifecycle environments of the SDKWork fleet, in promotion order (`sdkwork-deploy-contract::AppPublishEnvironment`). `demo` is a real publish target, so a binding may be environment-scoped to it. */
export type AppPublishEnvironment = 'development' | 'test' | 'staging' | 'demo' | 'production';
