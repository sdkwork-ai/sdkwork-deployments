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
  // One React instance, whichever repo a module is authored in — same dedupe
  // list `apps/sdkwork-deployments-pc/vite.config.ts` carries.
  resolve: {
    dedupe: ["react", "react-dom", "react-router", "react-router-dom"],
  },
  server: { port: 5182, strictPort: false },
  preview: { port: 4182 },
  build: {
    outDir: resolveBrowserDistOutDir(
      resolveViteEnvironment(mode, process.env),
      resolveViteDeploymentProfile(mode, process.env),
    ),
    target: "es2022",
    sourcemap: true,
  },
}));
