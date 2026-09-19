import type { CompositionKey } from './composition-key';

export interface AppVariantDefinition {
  key: CompositionKey;
  label: string;
  clientClass?: 'DESKTOP' | 'MOBILE' | 'TABLET' | 'TV' | 'BOT' | 'OTHER';
  priority?: number;
}
