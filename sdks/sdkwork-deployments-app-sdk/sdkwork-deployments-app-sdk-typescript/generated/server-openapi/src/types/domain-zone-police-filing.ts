/** Root-domain public security (police) filing information. Every member is optional. Text is trimmed, blank text is stored as NULL, and an object whose normalized members are all absent clears the filing. Record URLs must be absolute HTTP or HTTPS URLs; no date or future-time validation applies. */
export interface DomainZonePoliceFiling {
  /** Public security filing record number. Blank text is treated as absent. */
  recordNumber?: string;
  /** Absolute HTTP or HTTPS URL of the public security filing record. Blank text is treated as absent. */
  recordUrl?: string;
}
