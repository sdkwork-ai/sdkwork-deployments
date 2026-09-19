---
name: sdkwork-app-deployments
description: Build, package, upload, release, and deploy an SDKWork application through its manifest and the canonical Deploy and Drive app SDK facades. Use when an agent must publish or redeploy an application, create or reuse a Deploy Site, upload an immutable artifact, create a release, start a deployment, verify deployment evidence, or perform an explicitly approved rollback.
---

# SDKWork App Deployments

Publish an application from its root without guessing build outputs or bypassing SDK boundaries. Keep build declarations in `sdkwork.app.config.json`, concrete environment values in `etc/`, artifact bytes in Drive, and release/deployment state in SDKWork Deploy.

## Resolve The Application

1. Read the nearest `AGENTS.md`, `sdkwork.app.config.json`, applicable component `specs/`, and `etc/sdkwork.deployment.config.json`.
2. Resolve the application root, build target, lifecycle environment, deployment profile, Site identity, version, and deployment type from those authorities.
3. Require an explicit choice when more than one application root, Site, target, or profile matches.
4. Before a production deployment or first production Site creation, show the resolved target and obtain confirmation unless the user's current request already names that exact production action.

## Preserve Boundaries

- Use only build commands and outputs declared by `devApp.build.targets` in `sdkwork.app.config.json`. Do not infer `dist`, `build`, or framework defaults.
- Use the application's injected `@sdkwork/deployments-app-sdk` and `@sdkwork/drive-app-sdk` clients. Do not write raw HTTP wrappers, authorization headers, or local SDK forks.
- Use `createDeployApplicationPublisher` from `@sdkwork/deployments-app-sdk/application-publisher` for Site resolution, Drive upload, artifact registration, release creation, and deployment creation.
- Keep backend and app login profiles isolated. Never write tokens, passwords, presigned URLs, or secret values into tracked files or logs.
- Do not run the separate `sdkwork-dev-app` PlusApp registration flow unless the request also requires PlusApp catalog registration. That is a different publication boundary.
- Never rebuild an artifact during deployment. Deploy the immutable archive and SHA-256 digest that were reviewed.

## Publish Workflow

1. Preview the selected build target, output archive, version, environment, profile, Site selector, and deployment type.
2. Run the declared build command and package each declared directory output into its declared archive format.
3. Compute SHA-256 for the final archive and retain the digest as release evidence.
4. Resolve or create the Site without fuzzy matching. Reuse an existing Site by exact id, slug, or name; stop on ambiguous or incomplete lookup results.
5. Call the composed application publisher. It must execute this order: Site resolution, Drive archive upload, artifact registration, immutable release creation, and deployment creation.
6. Capture progress events and the returned Site, upload item/session, artifact, release, and deployment ids.
7. Retrieve the created deployment through the generated SDK. Report the raw backend status and timestamps; do not invent status meanings that are absent from the API contract.
8. Run the narrowest declared health or smoke verification for the selected environment. Do not claim success from deployment creation alone.

Read [references/publisher-flow.md](references/publisher-flow.md) before implementing or invoking the composed publisher.

## Failure And Recovery

- Stop at the failed stage and preserve all evidence returned by earlier completed stages.
- Reuse stable idempotency keys when retrying the same artifact, release, or deployment intent.
- Never create a second Application to work around ambiguous resolution.
- Roll back only when the user explicitly approves the Application and deployment id. There is no `deployments.rollback` operation: rollback is a forward fix that creates a new deployment against the previously released release id, and the app-API records lineage in the response-only `rollbackFromDeploymentId` field.

## Completion Evidence

Report:

- application root, build target, environment, and deployment profile
- archive name, byte size, and SHA-256 digest
- Application id and whether it was reused or created
- Drive upload item and session ids
- artifact, release, and deployment ids
- deployment status, timestamps, and health result
- retained rollback target, or why no rollback target is available

Treat missing ids, an incomplete upload response, an unverified deployment, or an unavailable health result as incomplete work.
