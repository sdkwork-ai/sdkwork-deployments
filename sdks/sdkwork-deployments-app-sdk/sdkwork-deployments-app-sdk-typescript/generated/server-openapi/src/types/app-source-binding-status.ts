/** Whether an uploaded source exists behind the spec. Deliberately separate from AppSourceSpecStatus: a spec is normally declared first and filled by the upload that follows, and collapsing the two would make "declared but not yet uploaded" indistinguishable from "switched off". */
export type AppSourceBindingStatus = 'EMPTY' | 'BOUND' | 'INVALID' | 'REVOKED';
