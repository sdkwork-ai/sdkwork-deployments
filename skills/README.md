# SDKWork Skills

Agent skills for `sdkwork-deployments` live in this directory:

```text
skills/<skill-name>/SKILL.md
```

A repository root is itself an authored module, so this is the module-scope container for
repository-wide workflows. Each skill follows `../../sdkwork-specs/AGENT_SKILLS_SPEC.md` and
`../../sdkwork-specs/SDKWORK_WORKSPACE_SPEC.md`. Names use lowercase kebab-case and equal the frontmatter
`name`. This file is the declaration index: it cites the standard and lists every skill with its
trigger and relative path.

Current skills:

| Skill | Use when | Path |
| --- | --- | --- |
| `sdkwork-app-deployments` | Any application operation: create/resolve/update, source (archive or Git directory), build/package/upload, release, deploy, verify, roll back, or manage app domains, env variables, health checks, platform targets, composition, and source specs | `skills/sdkwork-app-deployments/` |
| `sdkwork-app-scaffold` | Any app scaffold lifecycle operation: create/update the template listing, package from an archive or Git directory, upload the artifact, add a version, submit for review, track status, or withdraw | `skills/sdkwork-app-scaffold/` |

Add a skill by creating a lowercase kebab-case directory with a `SKILL.md` entrypoint and listing it
here. Do not place skills under `.sdkwork/`; that directory holds plugins and local state only.

## Runtime And Scripts

Both skills share one runtime: [`tools/agent-skills-sdk/`](../tools/agent-skills-sdk/README.md). It
owns login/logout/refresh, the SDK client bootstrap, the generic API dispatcher, and artifact upload.
It uses the npm-published SDKs installed in the target app; a local sdkwork-space TypeScript checkout
is a development fallback only. Each skill has a `scripts/run.mjs` launcher that delegates to it:

```bash
node skills/sdkwork-app-deployments/scripts/run.mjs doctor --cwd <app-root> --check-sdk-import
node skills/sdkwork-app-deployments/scripts/run.mjs login --cwd <app-root> --iam-base-url "$SDKWORK_IAM_APP_API_BASE_URL" --account you@sdkwork.com --password-stdin
node skills/sdkwork-app-deployments/scripts/run.mjs api --cwd <app-root> --operation apps.list
node skills/sdkwork-app-deployments/scripts/run.mjs upload --cwd <app-root> --app-id <id> --file dist/app.zip
```

The auth contract is
[`sdkwork-app-deployments/references/auth.md`](sdkwork-app-deployments/references/auth.md); the
generated Deploy operation map is
[`sdkwork-app-deployments/references/api-map.md`](sdkwork-app-deployments/references/api-map.md).

Verify with:

```bash
node ../sdkwork-specs/tools/check-agent-skills.mjs --root .
node tools/agent-skills-sdk/generate-operations.mjs --check
node tools/agent-skills-sdk/generate-api-map.mjs --check
node --test tools/agent-skills-sdk/runtime.test.mjs
```
