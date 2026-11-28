/** Locale → text. Keys are BCP 47 tags (`zh-CN`, `en-US`); a bare `zh`/`en` is widened on write. Stored as rows in deploy_app_capability_text, so adding a locale never requires a schema change. */
export type LocalizedText = Record<string, string>;
