import type { CompositionKey } from './composition-key';
import type { DriveDirectorySource } from './drive-directory-source';
import type { KnowledgebaseWikiSource } from './knowledgebase-wiki-source';

export interface AppResourceDefinition {
  key: CompositionKey;
  source: DriveDirectorySource | KnowledgebaseWikiSource;
}
