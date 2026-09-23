import { existsSync, mkdirSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const appRoot = resolve(root, "apps/sdkwork-deployments-pc");
const packages = [
  ["core", "pc", "runtime-core", {}],
  ["commons", "pc", "shared-ui", { react: "catalog:", "react-router-dom": "^7.15.0", "lucide-react": "catalog:" }],
  ["console-core", "app-console", "console-core", { "@sdkwork/deployments-app-sdk": "workspace:*", "@sdkwork/drive-app-sdk": "workspace:*", "@sdkwork/deployments-pc-commons": "workspace:*", "@sdkwork/sdk-common": "workspace:*", "@sdkwork/utils": "workspace:*", react: "catalog:" }, "sdkwork-deployments-app-sdk"],
  ["console-shell", "app-console", "console-shell", { "@sdkwork/deployments-pc-commons": "workspace:*", react: "catalog:" }],
  ["console-delivery", "app-console", "delivery", { "@sdkwork/deployments-pc-commons": "workspace:*", "@sdkwork/deployments-pc-console-core": "workspace:*", "lucide-react": "catalog:", react: "catalog:", "react-router-dom": "^7.15.0" }, null, [["domains", "Domains", "Domain verification and routing", null], ["certificates", "Certificates", "Managed and custom TLS certificates", null]]],
  ["console-publishing", "app-console", "publishing", { "@sdkwork/deployments-app-sdk": "workspace:*", "@sdkwork/deployments-pc-commons": "workspace:*", "@sdkwork/drive-app-sdk": "workspace:*", "@sdkwork/utils": "workspace:*", "lucide-react": "catalog:", react: "catalog:" }, "sdkwork-deployments-app-sdk", [["apps", "Apps", "Create and publish deploy_app applications", "deploy.apps.read"], ["artifacts", "Artifacts", "Drive-backed application packages", "deploy.artifacts.read"], ["releases", "Releases", "Immutable release versions", "deploy.releases.read"], ["deployments", "Deployments", "Rollout history and rollback", "deploy.deployments.read"]]],
  ["console-monitoring", "app-console", "monitoring", { "@sdkwork/deployments-pc-commons": "workspace:*" }, null, [["monitoring", "Monitoring", "Health check policy and status", "deploy.healthChecks.read"], ["configuration", "Configuration", "Environment variables and health checks", "deploy.apps.read"]]],
  ["admin-core", "backend-admin", "admin-core", { "@sdkwork/deployments-backend-sdk": "workspace:*", "@sdkwork/deployments-pc-commons": "workspace:*", "@sdkwork/sdk-common": "workspace:*", "@sdkwork/utils": "workspace:*", react: "catalog:" }, "sdkwork-deployments-backend-sdk"],
  ["admin-shell", "backend-admin", "admin-shell", { "@sdkwork/deployments-pc-commons": "workspace:*", react: "catalog:" }],
  ["admin-infrastructure", "backend-admin", "infrastructure", { "@sdkwork/deployments-pc-commons": "workspace:*" }, null, [["nginx", "Nginx", "Publishing gateway configuration", "deploy.nginx.write"]]],
  ["admin-nodes", "backend-admin", "nodes", { "@sdkwork/deployments-pc-commons": "workspace:*" }, null, [["clusters", "Clusters", "Node cluster of host nodes", "deploy.clusters.read"], ["nodes", "Nodes", "Host node inventory", "deploy.servers.read"]]],
  ["admin-audit", "backend-admin", "audit", { "@sdkwork/deployments-pc-commons": "workspace:*" }, null, [["audit", "Audit", "Publishing operator evidence", "deploy.auditLogs.read"]]],
];
for (const [id, surface, capability, dependencies, sdk, entries] of packages) {
  const directory = resolve(appRoot, "packages", `sdkwork-deployments-pc-${id}`); mkdirSync(resolve(directory, "src"), { recursive: true }); mkdirSync(resolve(directory, "specs"), { recursive: true });
  const publicExports = id === "console-delivery" ? ["src/index.ts", "src/management.ts"] : ["src/index.ts"];
  const isCore = id === "core" || id.endsWith("-core");
  const packageExports = Object.fromEntries(publicExports.map((entry, index) => { const key = index === 0 ? "." : `./${entry.slice(4, -3)}`; const target = `./${entry}`; return [key, { types: target, import: target, default: target }]; }));
  if (isCore) {
    // Standard core-package surface: sdk, modules, host, session, composition.
    for (const subpath of ["sdk", "modules", "host", "session", "composition"]) {
      packageExports[`./${subpath}`] = { types: `./src/${subpath}.ts`, import: `./src/${subpath}.ts`, default: `./src/${subpath}.ts` };
    }
    packageExports["./composition"].types = "./src/composition/index.ts";
    packageExports["./composition"].import = "./src/composition/index.ts";
    packageExports["./composition"].default = "./src/composition/index.ts";
    mkdirSync(resolve(directory, "src/composition"), { recursive: true });
    if (!existsSync(resolve(directory, "src/composition/index.ts"))) {
      writeFileSync(resolve(directory, "src/composition/index.ts"), 'export {} from "../index.ts";\n', "utf8");
    }
  }
  // React source in these packages is compiled source-level by the sdkwork
  // federation (birdcoder2 tsconfig.base.json paths), so the package itself
  // must carry @types/react or its imports type-check as implicit any
  // (TS7016/TS2786) in the host workspace build. The catalog resolves per
  // consuming workspace: 18 line in birdcoder2, 19 line standalone.
  const devDependencies = dependencies.react ? { "@types/react": "catalog:" } : undefined;
  writeJson(resolve(directory, "package.json"), { name: `@sdkwork/deployments-pc-${id}`, version: "0.1.0", private: true, type: "module", main: "./src/index.ts", exports: packageExports, dependencies, ...(devDependencies ? { devDependencies } : {}), sdkwork: { applicationCode: "deployments", architecture: "pc-react", capability, surface, managedBy: "tools/materialize_deployments_pc.mjs" } });
  const layerRole = id === "core" ? "frontend-core" : id.endsWith("-core") ? "frontend-core" : id === "commons" ? "frontend-commons" : id.endsWith("-shell") ? "frontend-shell" : "frontend-feature";
  writeJson(resolve(directory, "specs/component.spec.json"), { schemaVersion: 1, kind: "sdkwork.component.spec", component: { name: `@sdkwork/deployments-pc-${id}`, displayName: `SDKWork Deployments PC ${capability}`, version: "0.1.0", type: "node-package", root: `sdkwork-deployments/apps/sdkwork-deployments-pc/packages/sdkwork-deployments-pc-${id}`, domain: "deployment", capability, surface, languages: ["typescript"], generated: false, private: true, status: "active", manifests: ["package.json", "specs/component.spec.json"] }, canonicalSpecs: [{ file: "COMPONENT_SPEC.md", path: "../../../../../sdkwork-specs/COMPONENT_SPEC.md", purpose: "Component contract." }, { file: "APP_PC_ARCHITECTURE_SPEC.md", path: "../../../../../sdkwork-specs/APP_PC_ARCHITECTURE_SPEC.md", purpose: "PC package boundaries." }, { file: "APP_SDK_INTEGRATION_SPEC.md", path: "../../../../../sdkwork-specs/APP_SDK_INTEGRATION_SPEC.md", purpose: "SDK integration." }, { file: "DRIVE_SPEC.md", path: "../../../../../sdkwork-specs/DRIVE_SPEC.md", purpose: "Drive upload ownership." }, { file: "TEST_SPEC.md", path: "../../../../../sdkwork-specs/TEST_SPEC.md", purpose: "Verification." }], contracts: { layerRole, publicExports, runtimeEntrypoints: [], routeManifest: null, sdkClients: [], sdkDependencies: sdk ? [{ workspace: sdk, surface: surface === "backend-admin" ? "backend-api" : "app-api", credentialMode: surface === "backend-admin" ? "authenticated-backend-admin" : "authenticated-app-api" }] : [], providedPorts: [], requiredPorts: [], permissionComposition: isCore ? { inheritanceMode: "module-catalog-with-overrides", moduleCatalogRefs: sdk ? [{ moduleId: "deployments", manifestRef: "../../../../specs/iam.module.manifest.json", inheritPermissions: true, inheritRoles: true }] : [], consumerPolicy: { forbidLocalPermissionCatalogForDependencyDomains: true, allowExplicitOverridesOnly: true } } : { inheritanceMode: "openapi-with-explicit-ui-hints", routePermissionHints: { inheritFromOpenApi: true, overrides: [] }, consumerPolicy: { forbidLocalPermissionCatalogForDependencyDomains: true, allowFrontendHintsWithoutServerDuplication: true } }, events: [], configKeys: [], permissions: entries?.map((entry) => entry[3]).filter(Boolean) ?? [] }, integration: { authority: "Root SDKWork specs remain authoritative.", dependencyPolicy: "Consume sibling packages through public exports only.", sdkPolicy: surface === "backend-admin" ? "Backend SDK access is isolated behind admin-core." : "App and Drive SDK access is isolated behind console-core." }, verification: { commands: ["pnpm --dir apps/sdkwork-deployments-pc typecheck", "pnpm --dir apps/sdkwork-deployments-pc test"] }, metadata: { managedBy: "tools/materialize_deployments_pc.mjs", standardVersion: "2026-07-24" } });
  writeFileSync(resolve(directory, "specs/README.md"), `# ${capability}\n\nOwns the ${capability} capability on the ${surface} surface. Canonical SDKWork standards remain authoritative.\n`, "utf8");
  if (entries) { const rows = entries.map(([resource, label, description, permission], index) => { const permissionPart = permission ? `, permission: "${permission}"` : ""; return `    { resource: "${resource}", label: "${label}", description: "${description}"${permissionPart}, order: ${index + 1} }`; }).join(",\n"); writeFileSync(resolve(directory, "src/module.ts"), `import type { DeploymentsPcModuleDefinition } from "@sdkwork/deployments-pc-commons";\nexport const deploymentsModule = { id: "${capability}", label: "${capability.replaceAll("-", " ")}", surface: "${surface}", entries: [\n${rows}\n] } as const satisfies DeploymentsPcModuleDefinition;\n`, "utf8"); const indexExports = id === "console-publishing" ? 'export { deploymentsModule } from "./module.ts";\nexport * from "./publish.ts";\n' : "export { deploymentsModule } from \"./module.ts\";\n"; writeFileSync(resolve(directory, "src/index.ts"), indexExports, "utf8"); }
}
function writeJson(path, value) { writeFileSync(path, `${JSON.stringify(value, null, 2)}\n`, "utf8"); }
