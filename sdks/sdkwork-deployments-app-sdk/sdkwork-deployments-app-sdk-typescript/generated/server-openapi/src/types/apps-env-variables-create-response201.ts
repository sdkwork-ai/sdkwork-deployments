import type { EnvVariableResponse } from './env-variable-response';

export interface AppsEnvVariablesCreateResponse201 {
  code: 0;
  data: unknown & { item: EnvVariableResponse; };
  /** Server-owned request correlation id. */
  traceId: string;
}
