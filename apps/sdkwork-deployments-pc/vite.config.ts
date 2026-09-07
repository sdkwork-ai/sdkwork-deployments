import { resolveViteEnvironment, resolveLucideReactEntry } from '../../../sdkwork-specs/tools/vite-runtime-profile.mjs';
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
