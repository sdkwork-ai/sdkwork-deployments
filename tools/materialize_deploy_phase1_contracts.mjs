import fs from "node:fs";
import path from "node:path";
import { parse as parseYaml } from "yaml";
import { migrateOpenApiDocument } from "../../sdkwork-specs/tools/lib/migrate-openapi-legacy-envelope.mjs";

const root = process.cwd();

const surfaces = [
  {
    yamlPath: "apis/app-api/deploy/openapi.yaml",
    jsonAuthorityPath: "apis/app-api/deploy/deploy-app-api.openapi.json",
    sdkJsonPath: "sdks/sdkwork-deployments-app-sdk/openapi/deploy-app-api.openapi.json",
    sdkGenerationInputPath: "sdks/sdkwork-deployments-app-sdk/openapi/deploy-app-api.sdkgen.json",
    routeManifestPath:
      "sdks/_route-manifests/app-api/sdkwork-routes-deploy-app-api.route-manifest.json",
    crateDir: "crates/sdkwork-routes-deploy-app-api",
    manifestFn: "app_route_manifest",
    routePackageName: "sdkwork-routes-deploy-app-api",
    surface: "app-api",
    apiAuthority: "sdkwork-deploy-app-api",
    sdkFamily: "sdkwork-deployments-app-sdk",
    consumerPackageName: "@sdkwork/deployments-app-sdk",
    transportPackageName: "sdkwork-deployments-app-sdk-generated-typescript",
    sdkTarget: "app",
    prefix: "/app/v3/api",
    domainTag: "deploy",
    sdkDependencies: [
      {
        workspace: "sdkwork-drive-app-sdk",
        role: "drive-uploader-capability",
        required: true,
        dependencyMode: "consumer-sdk",
        apiPrefix: "/app/v3/api",
        apiAuthority: "sdkwork-drive-app-api",
        generatedTransportImportPolicy: "forbidden",
        packageByLanguage: {
          typescript: "@sdkwork/drive-app-sdk",
        },
      },
    ],
    dependencyApiExports: [
      {
        workspace: "sdkwork-drive-app-sdk",
        sdkFamily: "sdkwork-drive-app-sdk",
        apiAuthority: "sdkwork-drive-app-api",
        surface: "app-api",
        apiPrefix: "/app/v3/api",
        exportMode: "composed-wrapper",
        visibility: "app",
        methods: ["uploader.uploadArchive"],
        packageExport: "./application-publisher",
        runtimeRequired: true,
      },
    ],
  },
  {
    yamlPath: "apis/backend-api/deploy/openapi.yaml",
    jsonAuthorityPath: "apis/backend-api/deploy/deploy-backend-api.openapi.json",
    sdkJsonPath: "sdks/sdkwork-deployments-backend-sdk/openapi/deploy-backend-api.openapi.json",
    sdkGenerationInputPath: "sdks/sdkwork-deployments-backend-sdk/openapi/deploy-backend-api.sdkgen.json",
    routeManifestPath:
      "sdks/_route-manifests/backend-api/sdkwork-routes-deploy-backend-api.route-manifest.json",
    crateDir: "crates/sdkwork-routes-deploy-backend-api",
    manifestFn: "backend_route_manifest",
    routePackageName: "sdkwork-routes-deploy-backend-api",
    surface: "backend-api",
    apiAuthority: "sdkwork-deploy-backend-api",
    sdkFamily: "sdkwork-deployments-backend-sdk",
    consumerPackageName: "@sdkwork/deployments-backend-sdk",
    transportPackageName: "sdkwork-deployments-backend-sdk-generated-typescript",
    sdkTarget: "backend",
    prefix: "/backend/v3/api",
    domainTag: "deploy",
  },
];

function writeText(relativePath, content) {
  const target = path.join(root, relativePath);
  fs.mkdirSync(path.dirname(target), { recursive: true });
  fs.writeFileSync(target, content.replace(/\r\n/g, "\n"), "utf8");
  console.log(`wrote ${relativePath}`);
}

function writeJson(relativePath, value) {
  writeText(relativePath, `${JSON.stringify(value, null, 2)}\n`);
}

function assertWellFormedUnicode(value, sourcePath, pathSegments = []) {
  if (typeof value === "string") {
    if (/[\uD800-\uDFFF]/.test(value)) {
      const pointer = pathSegments.length === 0 ? "/" : `/${pathSegments.join("/")}`;
      throw new Error(`${sourcePath}${pointer} contains an unpaired UTF-16 surrogate`);
    }
    return;
  }
  if (Array.isArray(value)) {
    value.forEach((item, index) =>
      assertWellFormedUnicode(item, sourcePath, [...pathSegments, String(index)]),
    );
    return;
  }
  if (value && typeof value === "object") {
    for (const [key, item] of Object.entries(value)) {
      assertWellFormedUnicode(item, sourcePath, [...pathSegments, key]);
    }
  }
}

function sdkManifest(profile, openapi) {
  const familyRoot = `sdks/${profile.sdkFamily}/`;
  for (const ownedPath of [profile.sdkJsonPath, profile.sdkGenerationInputPath]) {
    if (!ownedPath.startsWith(familyRoot)) {
      throw new Error(`${ownedPath} is outside ${familyRoot}`);
    }
  }
  const familyOpenApiPath = profile.sdkJsonPath.slice(familyRoot.length);
  const generationInputSpec = profile.sdkGenerationInputPath.slice(familyRoot.length);
  const typescriptRoot = `${profile.sdkFamily}-typescript`;
  const transportRoot = `${typescriptRoot}/generated/server-openapi`;
  return {
    schemaVersion: 1,
    sdkFamily: profile.sdkFamily,
    sdkName: profile.sdkFamily,
    packageName: profile.consumerPackageName,
    transportPackageName: profile.transportPackageName,
    typescript: {
      composedRoot: typescriptRoot,
      composedEntry: `${typescriptRoot}/src/index.ts`,
      transportRoot,
      transportEntry: `${transportRoot}/src/index.ts`,
    },
    workspace: profile.sdkFamily,
    title: openapi.info?.title ?? profile.sdkFamily,
    apiVersion: openapi.info?.version ?? "0.1.0",
    openapiVersion: openapi.openapi,
    authoritySpec: familyOpenApiPath,
    generationInputSpec,
    derivedSpecs: { default: generationInputSpec },
    sdkOwner: "sdkwork-deploy",
    apiAuthority: profile.apiAuthority,
    ownerOnlyOperationCount: extractRoutes(openapi, profile).length,
    standardProfile: "sdkwork-v3",
    discoverySurface: {
      sdkTarget: profile.sdkTarget,
      apiPrefix: profile.prefix,
      schemaUrl: `${profile.prefix}/openapi.json`,
      generatedProtocols: ["http-openapi"],
      manualTransports: [],
    },
    sdkDependencies: profile.sdkDependencies ?? [],
    dependencyApiExports: profile.dependencyApiExports ?? [],
    languages: [
      {
        language: "typescript",
        workspace: typescriptRoot,
        generationState: "materialized",
        releaseState: "not_published",
        packagePath: transportRoot,
        manifestPath: `${transportRoot}/package.json`,
        version: "0.1.0",
        description: `Generator-owned TypeScript transport SDK for ${openapi.info?.title ?? profile.apiAuthority}.`,
        generatedPath: transportRoot,
        consumerPackageName: profile.consumerPackageName,
        transportPackageName: profile.transportPackageName,
      },
    ],
    metadata: {
      managedBy: "tools/materialize_deploy_phase1_contracts.mjs",
      standardProfile: "sdkwork-v3",
      supportedLanguageSubset: ["typescript"],
    },
    openApiPath: familyOpenApiPath,
    surface: profile.surface,
  };
}

function enrichOpenApi(openapi, profile) {
  const enriched = structuredClone(openapi);
  for (const [pathKey, pathItem] of Object.entries(enriched.paths ?? {})) {
    for (const [method, operation] of Object.entries(pathItem ?? {})) {
      if (!["get", "post", "put", "patch", "delete"].includes(method)) {
        continue;
      }
      operation["x-sdkwork-api-surface"] = profile.surface;
      operation["x-sdkwork-owner"] = "sdkwork-deploy";
      operation["x-sdkwork-api-authority"] = profile.apiAuthority;
      operation["x-sdkwork-request-context"] = "WebRequestContext";
      operation["x-sdkwork-auth-mode"] =
        operation["x-sdkwork-auth-mode"] ?? "dual-token";
      if (operation["x-sdkwork-permission"] === false) {
        // Explicit no-permission marker: authentication still follows
        // x-sdkwork-auth-mode, but no specific permission is required.
        delete operation["x-sdkwork-permission"];
      } else if (!operation["x-sdkwork-permission"] && operation.operationId) {
        // The verb is carried by the operationId's **last** segment, not its
        // second. A nested resource puts a sub-domain in the middle
        // (`apps.sourceSpecs.list`), and reading that segment as the action
        // classified every nested read as a write — nine `list`/`retrieve`
        // routes (apps.domains, apps.envVariables, apps.healthChecks,
        // apps.sourceSpecs, domainZones.hostnames, certificates.renewals,
        // channels.rollouts) demanded `write` from read-only callers.
        const segments = operation.operationId.split(".");
        const action = segments[segments.length - 1];
        const verb = action?.includes("list") || action?.includes("retrieve")
          ? "read"
          : "write";
        operation["x-sdkwork-permission"] = `deploy.${segments[0]}.${verb}`;
      }
      if (
        method === "post" &&
        (operation.operationId?.includes("create") ||
          operation.operationId?.includes("rollback") ||
          operation.operationId?.includes("reload") ||
          operation.operationId?.includes("deploy") ||
          operation.operationId?.includes("verify"))
      ) {
        operation["x-sdkwork-idempotent"] = true;
      }
    }
  }
  inlineReusableErrorResponses(enriched);
  return enriched;
}

function inlineReusableErrorResponses(openapi) {
  const reusableResponses = openapi.components?.responses ?? {};
  for (const pathItem of Object.values(openapi.paths ?? {})) {
    for (const [method, operation] of Object.entries(pathItem ?? {})) {
      if (!["get", "post", "put", "patch", "delete"].includes(method)) continue;
      for (const [statusCode, response] of Object.entries(operation.responses ?? {})) {
        if (Number(statusCode) < 400 || typeof response?.$ref !== "string") continue;
        const prefix = "#/components/responses/";
        if (!response.$ref.startsWith(prefix)) continue;
        const reusable = reusableResponses[response.$ref.slice(prefix.length)];
        if (reusable) operation.responses[statusCode] = structuredClone(reusable);
      }
    }
  }
}

function extractRoutes(openapi, profile) {
  const routes = [];
  for (const [pathKey, pathItem] of Object.entries(openapi.paths ?? {})) {
    for (const [method, operation] of Object.entries(pathItem ?? {})) {
      if (!["get", "post", "put", "patch", "delete"].includes(method)) {
        continue;
      }
      routes.push({
        method: method.toUpperCase(),
        path: pathKey,
        operationId: operation.operationId,
        tags: operation.tags ?? [profile.domainTag],
        auth: {
          mode: operation["x-sdkwork-auth-mode"] ?? "dual-token",
          required: true,
        },
        handler: { module: "crate::routes", name: null },
        ownership: {
          owner: "sdkwork-deploy",
          apiAuthority: profile.apiAuthority,
        },
        requestContext: "WebRequestContext",
        apiSurface: profile.surface,
        permission: operation["x-sdkwork-permission"] ?? null,
        idempotent: operation["x-sdkwork-idempotent"] === true,
      });
    }
  }
  return routes;
}

function httpRouteAuthHelper(authMode) {
  return authMode === "api-key" ? "api_key" : "dual_token";
}

function httpMethodRust(method) {
  return { GET: "Get", POST: "Post", PATCH: "Patch", PUT: "Put", DELETE: "Delete" }[
    method
  ];
}

function writeHttpRouteManifestRust(crateDir, fnName, routes) {
  const lines = [
    "// @generated by tools/materialize_deploy_phase1_contracts.mjs - do not edit",
    "",
    "use sdkwork_web_core::{HttpMethod, HttpRoute, HttpRouteManifest};",
    "",
    "const HTTP_ROUTES: &[HttpRoute] = &[",
  ];
  for (const route of routes) {
    const auth = httpRouteAuthHelper(route.auth?.mode ?? "dual-token");
    const suffix = [
      route.permission ? `.with_required_permission("${route.permission}")` : "",
      route.idempotent ? ".with_idempotent(true)" : "",
    ].join("");
    lines.push(`    HttpRoute::${auth}(`);
    lines.push(`        HttpMethod::${httpMethodRust(route.method)},`);
    lines.push(`        "${route.path}",`);
    lines.push(`        "${route.tags[0] ?? "deploy"}",`);
    lines.push(`        "${route.operationId}",`);
    lines.push(`    )${suffix},`);
  }
  lines.push("];", "", `pub fn ${fnName}() -> HttpRouteManifest {`, "    HttpRouteManifest::new(HTTP_ROUTES)", "}", "");
  writeText(`${crateDir}/src/http_route_manifest.rs`, lines.join("\n"));
}

const materializedOpenApis = new Map();

for (const profile of surfaces) {
  const yaml = parseYaml(fs.readFileSync(path.join(root, profile.yamlPath), "utf8"));
  assertWellFormedUnicode(yaml, profile.yamlPath);
  const openapi = migrateOpenApiDocument(enrichOpenApi(yaml, profile));
  writeJson(profile.jsonAuthorityPath, openapi);
  writeJson(profile.sdkJsonPath, openapi);
  writeJson(profile.sdkGenerationInputPath, openapi);
  materializedOpenApis.set(profile.apiAuthority, openapi);
  const routes = extractRoutes(openapi, profile);
  writeJson(profile.routeManifestPath, {
    schemaVersion: 1,
    kind: "sdkwork.route.manifest",
    packageName: profile.routePackageName,
    surface: profile.surface,
    owner: "sdkwork-deploy",
    domain: "platform",
    capability: "deploy",
    apiAuthority: profile.apiAuthority,
    sdkFamily: profile.sdkFamily,
    prefix: profile.prefix,
    source: {
      crateRoot: profile.crateDir,
      crateImport: profile.routePackageName.replaceAll("-", "_"),
      openApiAuthority: profile.sdkJsonPath,
    },
    routes,
  });
  writeHttpRouteManifestRust(profile.crateDir, profile.manifestFn, routes);
}

writeJson("apis/authority-manifest.json", {
  schemaVersion: 1,
  kind: "sdkwork.api.authority.manifest",
  surfaces: surfaces.map((profile) => ({
    authorityPath: profile.jsonAuthorityPath,
    sdkPath: profile.sdkJsonPath,
  })),
});

for (const profile of surfaces) {
  const openapi = materializedOpenApis.get(profile.apiAuthority);
  if (!openapi) {
    throw new Error(`missing materialized OpenAPI for ${profile.apiAuthority}`);
  }
  writeJson(`sdks/${profile.sdkFamily}/sdk-manifest.json`, sdkManifest(profile, openapi));
}

// The route-manifest Rust sources are materialized with hand formatting;
// normalize them with rustfmt so `cargo fmt --check` stays green.
const { spawnSync } = await import("node:child_process");
const format = spawnSync("cargo", ["fmt", "--", "--check"], { encoding: "utf8" });
if (format.status !== 0) {
  const apply = spawnSync("cargo", ["fmt"], { encoding: "utf8" });
  if (apply.status !== 0) {
    throw new Error(`cargo fmt after materialization failed: ${apply.stderr}`);
  }
  console.log("deploy phase-1 contracts materialized (formatted)");
} else {
  console.log("deploy phase-1 contracts materialized");
}
