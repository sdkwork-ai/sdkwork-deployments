# Deployments PC Source Configuration

`sdkwork.deployment.config.json` is the configuration entrypoint. It selects the browser runtime target and the `cloud` deployment profile; concrete environment values are tracked under `browser/`.

Supported environments are `development`, `test`, `staging`, and `production`. Their typed browser profiles are `browser/runtime-env.<environment>.json`; `agent.app.config.json` remains application identity metadata and is not a runtime-value source.

The schema authority is `CONFIG_SPEC.md`, `SOURCE_CONFIG_SPEC.md`, and `ENVIRONMENT_SPEC.md` in `sdkwork-specs`. Local overrides must use ignored local files or process-local environment input and must never modify tracked profiles. Secrets, access tokens, refresh tokens, API keys, certificate private keys, presigned URLs, and provider metadata are forbidden in browser runtime configuration; authenticated state comes from IAM and the shared TokenManager.

`scripts/materialize-runtime-env.mjs --environment <environment>` materializes the selected source to `public/runtime-env.json` before Vite starts or builds. Validate this root with:

```powershell
node ../../../sdkwork-specs/tools/check-source-config-standard.mjs --root .
```
