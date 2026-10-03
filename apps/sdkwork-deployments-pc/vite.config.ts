import { resolveViteEnvironment } from '../../../sdkwork-specs/tools/vite-runtime-profile.mjs';
import { resolveBrowserDistOutDir } from '../../../sdkwork-specs/tools/browser-dist-layout.mjs';
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";



function resolveViteDeploymentProfile(mode: string | undefined, processEnv = process.env) {
  const profileMatch = /^(standalone|cloud)\./u.exec(mode ?? '');
  return profileMatch?.[1]
    ?? processEnv.SDKWORK_DEPLOYMENT_PROFILE
    ?? 'standalone';
}

export default defineConfig(({ mode }) => ({
  plugins: [react()],
  // One React instance, whichever repo a module is authored in.
  //
  // This app composes `@sdkwork/ui-pc-react` (DataTable et al.) straight from
  // the `sdkwork-ui` repo through a workspace link, and every capability package
  // under `packages/` imports `react` itself. Without a dedupe list each importer
  // resolves `react` from *its own* repo's store, so the framework component and
  // the component that renders it end up on two different React copies and the
  // shared internals come back null — reported as "Invalid hook call" from
  // whichever hook runs first (`cross-repo-duplicate-react-vitest`).
  //
  // The list is the same one `apps/sdkwork-webserver-pc/vite.config.ts` carries,
  // which is the closest sibling: same workspace-link shape, same framework.
  resolve: {
    dedupe: ["react", "react-dom", "react-router", "react-router-dom"],
  },
  server: { port: 5181, strictPort: false },
  preview: { port: 4181 },
  build: {
    outDir: resolveBrowserDistOutDir(
      resolveViteEnvironment(mode, process.env),
      resolveViteDeploymentProfile(mode, process.env),
    ),
    target: "es2022",
    sourcemap: true,
  },
}));
