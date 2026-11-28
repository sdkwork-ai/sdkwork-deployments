/** Platform lifecycle vocabulary. `published` requires a published revision and timestamp; editing a published document returns it to `draft` while the published revision stays live. */
export type CapabilityStatus = 'draft' | 'pending_review' | 'approved' | 'published' | 'deprecated' | 'retired';
