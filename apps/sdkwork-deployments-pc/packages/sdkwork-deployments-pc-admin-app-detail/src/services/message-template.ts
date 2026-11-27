/**
 * Message template expansion.
 *
 * Catalog copy carries placeholders such as `{count}` / `{range}`. Rendering
 * them is a pure string operation, kept here so every region of the detail
 * surface expands the same catalog entry the same way.
 */
export function templateMessage(value: string, replacements: Record<string, string>): string {
  return Object.entries(replacements).reduce(
    (result, [key, replacement]) => result.replace(`{${key}}`, replacement),
    value,
  );
}
