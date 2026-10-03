# apps/

Application: sdkwork-deploy
Status: active
Owner: SDKWork maintainers
Specs: APPLICATION_SPEC.md, SDKWORK_WORKSPACE_SPEC.md

## Primary App Surface

The repository root is the primary runnable app surface.
The repository root `sdkwork.app.config.json` governs the primary application manifest.

## Directory Index

| Directory | Surface role | Runnable | Purpose | Entry |
| --- | --- | --- | --- | --- |
| sdkwork-deployments-pc | pc | yes | SDKWork Deployments Console and backend-admin publishing control plane | [README](sdkwork-deployments-pc/README.md) |
| sdkwork-deployments-h5 | h5 | yes | Mobile web surface of the app template marketplace (browse, acquire, my templates) | [README](sdkwork-deployments-h5/README.md) |
| sdkwork-deployments-flutter | flutter | yes | Flutter surface of the app template marketplace over the generated Dart SDK | [README](sdkwork-deployments-flutter/README.md) |
| sdkwork-deployments-mini-program | mini-program | yes | WeChat mini-program surface of the app template marketplace | [README](sdkwork-deployments-mini-program/README.md) |

## Allowed Content

- Selected language/architecture application roots with `README.md`, `AGENTS.md`, `.sdkwork/`, and `specs/` when authored packages exist.
- Architecture-local `packages/`, `config/`, `src/`, `lib/`, `App/`, or `entry/` directories required by the owning architecture standard.

## Forbidden Content

- Repository-root API contracts, generated SDK workspaces, Rust crates, or deployment descriptors moved under `apps/`.
- Runtime secrets, user-private state, generated SDK transport output, or cross-application copied business logic.

## Related Specs

- `../sdkwork-specs/APPLICATION_SPEC.md`
- `../sdkwork-specs/SDKWORK_WORKSPACE_SPEC.md`
- `../sdkwork-specs/APP_CLIENT_ARCHITECTURE_ALIGNMENT_SPEC.md`

## Verification

```bash
node ../sdkwork-specs/tools/check-apps-directory-index.mjs --root .
```
