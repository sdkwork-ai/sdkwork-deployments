/** Root-domain ICP filing information. Every member is optional. Text is trimmed, blank text is stored as NULL, and an empty object clears the filing. */
export interface DomainZoneFiling {
  /** ICP filing record number. Blank text is treated as absent. */
  recordNumber?: string;
  /** Registered entity name on the ICP filing. Blank text is treated as absent. */
  entityName?: string;
  /** Website name on the ICP filing. Blank text is treated as absent. */
  websiteName?: string;
  /** ICP review date in ISO YYYY-MM-DD form. It must be a real calendar date and cannot be in the future. */
  reviewDate?: string;
}
