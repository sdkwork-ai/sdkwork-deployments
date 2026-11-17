/**
 * Resolve TypeScript source packages from their public package entrypoints.
 *
 * SDKWork SDK packages export `src/index.ts`. Node 24 strips types, but it does not
 * resolve extensionless relative imports, `.js` specifiers that point at `.ts`
 * sources, or directory imports whose `index` is a `.ts` file. This hook supplies
 * those resolutions so a skill script can import a public package name instead of a
 * generated `dist` path.
 */
const RESOLVE_CODES = new Set(['ERR_MODULE_NOT_FOUND', 'ERR_UNSUPPORTED_DIR_IMPORT']);

export async function resolve(specifier, context, nextResolve) {
  try {
    return await nextResolve(specifier, context);
  } catch (error) {
    if (!RESOLVE_CODES.has(error?.code)) throw error;
    if (!context.parentURL || !specifier.startsWith('.')) throw error;

    const candidates = [];
    if (specifier.endsWith('.js')) {
      candidates.push(`${specifier.slice(0, -3)}.ts`);
    }
    candidates.push(`${specifier}.ts`, `${specifier}/index.ts`);

    let lastError = error;
    for (const candidate of candidates) {
      try {
        return await nextResolve(candidate, context);
      } catch (candidateError) {
        lastError = candidateError;
      }
    }
    throw lastError;
  }
}
