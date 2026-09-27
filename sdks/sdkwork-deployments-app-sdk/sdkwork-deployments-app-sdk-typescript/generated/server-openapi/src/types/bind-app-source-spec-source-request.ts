import type { DriveDirectorySource } from './drive-directory-source';
import type { KnowledgebaseWikiSource } from './knowledgebase-wiki-source';

export interface BindAppSourceSpecSourceRequest {
  /** Where the uploaded bytes live. Described with the same vocabulary `apps.composition.update` uses for a resource, and resolved through the content provider before it is stored, so a spec can only ever carry a provider reference the delivery plane already accepted. */
  source: DriveDirectorySource | KnowledgebaseWikiSource;
}
