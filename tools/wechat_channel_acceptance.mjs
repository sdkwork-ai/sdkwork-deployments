#!/usr/bin/env node
/**
 * WeChat channel acceptance driver.
 *
 * Runs the commercial acceptance path against a configured environment:
 *
 *   IAM phase
 *     1. operator-platform connectivity check   (component ticket + token)
 *     2. create a pre-authorization link        (operator scan entry)
 *     3. list resource authorizations           (confirm the grant is active)
 *
 *   Deploy phase
 *     4. create a channel release               (draft record)
 *     5. run publish commands                   (upload / submit / release / ...)
 *
 * Every step is recorded in a JSON evidence file with the HTTP status and the
 * `x-sdkwork-trace-id`, so the report can be attached to a release review.
 *
 * Usage:
 *   node tools/wechat_channel_acceptance.mjs --dry-run
 *   node tools/wechat_channel_acceptance.mjs --phase iam
 *   node tools/wechat_channel_acceptance.mjs --phase all --output target/acceptance/run.json
 *
 * Environment:
 *   SDKWORK_ACCEPTANCE_IAM_BASE_URL        e.g. https://iam.example.com
 *   SDKWORK_ACCEPTANCE_IAM_BEARER          backend bearer (never logged)
 *   SDKWORK_ACCEPTANCE_DEPLOY_BASE_URL     e.g. https://deploy.example.com
 *   SDKWORK_ACCEPTANCE_DEPLOY_BEARER       app-api bearer (never logged)
 *   SDKWORK_ACCEPTANCE_OPERATOR_PLATFORM_ID  operator platform row id
 *   SDKWORK_ACCEPTANCE_APP_ID              Deploy application id
 *   SDKWORK_ACCEPTANCE_RELEASE_ID          optional existing release id
 *   SDKWORK_ACCEPTANCE_RESOURCE_LINK_ID    required when no release id is given
 *   SDKWORK_ACCEPTANCE_ENVIRONMENT         default production
 *   SDKWORK_ACCEPTANCE_CHANNEL             default wechat_mini_program
 *   SDKWORK_ACCEPTANCE_ACCESS_MODE         default operator_authorized
 *   SDKWORK_ACCEPTANCE_AUTHORIZATION_ID    used by publish commands
 *   SDKWORK_ACCEPTANCE_ACTIONS             default upload,submit_audit,release
 *   SDKWORK_ACCEPTANCE_ACTION_PAYLOAD      extra JSON merged into every publish body
 *
 * Exit codes: 0 success (or dry run), 1 a step failed, 2 configuration error.
 */
import { randomUUID } from "node:crypto";
import { mkdir, writeFile } from "node:fs/promises";
import { dirname } from "node:path";

const args = process.argv.slice(2);
const dryRun = args.includes("--dry-run");
const phaseArg = valueAfter("--phase") ?? "all";
const outputArg = valueAfter("--output") ?? defaultOutput();

if (!["iam", "deploy", "all"].includes(phaseArg)) {
  fail(2, `unknown phase ${phaseArg}; use iam, deploy, or all`);
}

const env = process.env;
const iamBase = trim(env.SDWORK_ACCEPTANCE_IAM_BASE_URL);
const deployBase = trim(env.SDWORK_ACCEPTANCE_DEPLOY_BASE_URL);
const evidence = {
  startedAt: new Date().toISOString(),
  dryRun,
  phase: phaseArg,
  steps: [],
};

const steps = [];
if (phaseArg === "iam" || phaseArg === "all") {
  steps.push(...iamSteps());
}
if (phaseArg === "deploy" || phaseArg === "all") {
  steps.push(...deploySteps());
}

if (steps.length === 0) {
  fail(2, "nothing to run for the selected phase");
}

if (dryRun) {
  for (const step of steps) {
    console.log(`[dry-run] ${step.name}: ${step.method} ${step.url}`);
  }
  evidence.steps = steps.map((step) => ({
    name: step.name,
    method: step.method,
    url: step.url,
    dryRun: true,
  }));
  evidence.finishedAt = new Date().toISOString();
  await writeEvidence(evidence, outputArg);
  console.log(`[dry-run] evidence written to ${outputArg}`);
  process.exit(0);
}

for (const step of steps) {
  const result = await execute(step);
  evidence.steps.push(result);
  const marker = result.ok ? "ok" : "FAILED";
  console.log(
    `[${marker}] ${step.name} ${result.status ?? "-"}${result.traceId ? ` trace=${result.traceId}` : ""}`,
  );
  if (!result.ok) {
    evidence.finishedAt = new Date().toISOString();
    await writeEvidence(evidence, outputArg);
    fail(1, `${step.name} failed: ${result.detail ?? "see evidence"}`);
  }
}

evidence.finishedAt = new Date().toISOString();
await writeEvidence(evidence, outputArg);
console.log(`acceptance passed; evidence written to ${outputArg}`);

function iamSteps() {
  const platformId = required("SDKWORK_ACCEPTANCE_OPERATOR_PLATFORM_ID");
  const base = required("SDKWORK_ACCEPTANCE_IAM_BASE_URL");
  return [
    {
      name: "iam.connectivity",
      method: "POST",
      url: `${base}/backend/v3/api/iam/oauth/operator_platforms/${platformId}/connectivity_checks`,
      bearer: required("SDKWORK_ACCEPTANCE_IAM_BEARER"),
      expect: (body) => body?.data?.item?.componentTokenAcquired === true,
      expectDetail: "componentTokenAcquired must be true",
    },
    {
      name: "iam.pre_authorization",
      method: "POST",
      url: `${base}/backend/v3/api/iam/oauth/operator_platforms/${platformId}/pre_authorizations`,
      bearer: required("SDKWORK_ACCEPTANCE_IAM_BEARER"),
      expect: (body) =>
        typeof body?.data?.item?.authorizationUrl === "string" ||
        typeof body?.data?.item?.url === "string",
      expectDetail: "pre-authorization URL is missing",
    },
    {
      name: "iam.resource_authorizations",
      method: "GET",
      url: `${base}/backend/v3/api/iam/oauth/resource_authorizations?page=1&pageSize=50`,
      bearer: required("SDKWORK_ACCEPTANCE_IAM_BEARER"),
      expect: (body) => Array.isArray(body?.data?.items),
      expectDetail: "resource authorization page is missing",
    },
  ];
}

function deploySteps() {
  const base = required("SDKWORK_ACCEPTANCE_DEPLOY_BASE_URL");
  const appId = required("SDKWORK_ACCEPTANCE_APP_ID");
  const bearer = required("SDKWORK_ACCEPTANCE_DEPLOY_BEARER");
  const releaseId = trim(env.SDWORK_ACCEPTANCE_RELEASE_ID);
  const steps = [];
  if (!releaseId) {
    steps.push({
      name: "deploy.channel_release.create",
      method: "POST",
      url: `${base}/app/v3/api/apps/${appId}/channel_releases`,
      bearer,
      body: {
        environment: trim(env.SDWORK_ACCEPTANCE_ENVIRONMENT) || "production",
        resourceLinkId: required("SDKWORK_ACCEPTANCE_RESOURCE_LINK_ID"),
        channel: trim(env.SDWORK_ACCEPTANCE_CHANNEL) || "wechat_mini_program",
        accessMode: trim(env.SDWORK_ACCEPTANCE_ACCESS_MODE) || "operator_authorized",
      },
      expect: (body) => typeof body?.data?.item?.id === "string",
      expectDetail: "channel release id is missing",
      capture: (body) => ({ releaseId: body.data.item.id }),
    });
  }
  const actions = (trim(env.SDWORK_ACCEPTANCE_ACTIONS) || "upload,submit_audit,release")
    .split(",")
    .map((value) => value.trim())
    .filter(Boolean);
  const extra = parseJson(env.SDWORK_ACCEPTANCE_ACTION_PAYLOAD, "SDKWORK_ACCEPTANCE_ACTION_PAYLOAD");
  const authorizationId = trim(env.SDWORK_ACCEPTANCE_AUTHORIZATION_ID);
  for (const action of actions) {
    steps.push({
      name: `deploy.publish.${action}`,
      method: "POST",
      url: `${base}/app/v3/api/apps/${appId}/channel_releases/__RELEASE__/publish_commands`,
      bearer,
      idempotencyKey: randomUUID(),
      body: () => ({
        action,
        ...(authorizationId ? { authorizationId } : {}),
        ...(extra ?? {}),
      }),
      expect: (body) => body?.data?.item?.releaseStatus !== undefined,
      expectDetail: "publish command response is missing releaseStatus",
    });
  }
  return steps;
}

async function execute(step) {
  const headers = {
    authorization: `Bearer ${step.bearer}`,
    accept: "application/json",
  };
  if (step.idempotencyKey) {
    headers["idempotency-key"] = step.idempotencyKey;
  }
  const body = typeof step.body === "function" ? step.body() : step.body;
  if (body !== undefined) {
    headers["content-type"] = "application/json";
  }
  let url = step.url;
  if (url.includes("__RELEASE__")) {
    const releaseId = trim(env.SDWORK_ACCEPTANCE_RELEASE_ID) ?? evidence.steps.find((entry) => entry.releaseId)?.releaseId;
    if (!releaseId) {
      return {
        name: step.name,
        method: step.method,
        url,
        ok: false,
        detail: "no release id: set SDKWORK_ACCEPTANCE_RELEASE_ID or create one first",
      };
    }
    url = url.replace("__RELEASE__", releaseId);
  }
  let response;
  try {
    response = await fetch(url, {
      method: step.method,
      headers,
      body: body === undefined ? undefined : JSON.stringify(body),
      signal: AbortSignal.timeout(30_000),
    });
  } catch (error) {
    return { name: step.name, method: step.method, url, ok: false, detail: String(error) };
  }
  const traceId = response.headers.get("x-sdkwork-trace-id") ?? undefined;
  const text = await response.text();
  let parsed;
  try {
    parsed = text.length > 0 ? JSON.parse(text) : undefined;
  } catch {
    parsed = undefined;
  }
  const ok = response.ok && step.expect(parsed);
  const result = {
    name: step.name,
    method: step.method,
    url,
    status: response.status,
    traceId,
    ok,
  };
  if (!ok) {
    result.detail = parsed?.detail ?? step.expectDetail ?? text.slice(0, 512);
  }
  if (step.capture && ok) {
    Object.assign(result, step.capture(parsed));
  }
  return result;
}

function required(name) {
  const value = trim(env[name]);
  if (!value) {
    fail(2, `${name} is required`);
  }
  return value;
}

function trim(value) {
  return typeof value === "string" ? value.trim() : "";
}

function parseJson(value, name) {
  if (!value || !value.trim()) {
    return undefined;
  }
  try {
    return JSON.parse(value);
  } catch (error) {
    fail(2, `${name} is not valid JSON: ${error.message}`);
  }
}

function valueAfter(flag) {
  const index = args.indexOf(flag);
  return index >= 0 ? args[index + 1] : undefined;
}

function defaultOutput() {
  const stamp = new Date().toISOString().replace(/[:.]/g, "-");
  return `target/acceptance/wechat-channel-${stamp}.json`;
}

async function writeEvidence(report, path) {
  await mkdir(dirname(path), { recursive: true });
  await writeFile(path, `${JSON.stringify(report, null, 2)}\n`, "utf8");
}

function fail(code, message) {
  console.error(`error: ${message}`);
  process.exit(code);
}
