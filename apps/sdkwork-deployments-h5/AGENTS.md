# Repository Guidelines — sdkwork-deployments-h5

Read `../../AGENTS.md` (repository entrypoint) and `../../../sdkwork-specs/APP_H5_ARCHITECTURE_SPEC.md` before changes here.

## Boundaries

- `src/` is thin entry + composition only (bootstrap, auth gate, tab shell); capability code lives in `src/marketplace/`.
- SDK clients are constructed exactly once in `src/bootstrap/runtime.ts` and injected; views/services never import `createClient` and never call raw HTTP.
- The H5 surface consumes the app-api SDK only; the backend-admin SDK is forbidden here (mirror of the PC architecture-boundary test).
- User-facing copy is bilingual via `src/marketplace/i18n.ts` — English keys are the authority, `zh-CN` is typed against them.

## Verification

```bash
pnpm --dir apps/sdkwork-deployments-h5 typecheck
pnpm --dir apps/sdkwork-deployments-h5 test
node ../../sdkwork-specs/tools/check-application-layering.mjs --root ../..
node ../../sdkwork-specs/tools/check-apps-directory-index.mjs --root ../..
```
