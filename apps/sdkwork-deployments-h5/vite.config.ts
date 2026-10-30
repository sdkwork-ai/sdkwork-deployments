import { resolveViteEnvironment } from '../../../sdkwork-specs/tools/vite-runtime-profile.mjs';
import { resolveBrowserDistOutDir } from '../../../sdkwork-specs/tools/browser-dist-layout.mjs';
import { createSdkworkCredentialEntryBootstrapVitePlugin } from '../../../sdkwork-iam/apps/sdkwork-iam-common/packages/sdkwork-iam-credential-entry/src/vite.ts';
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

function resolveViteDeploymentProfile(mode: string | undefined, processEnv = process.env) {
  const profileMatch = /^(standalone|cloud)\./u.exec(mode ?? '');
  return profileMatch?.[1]
    ?? processEnv.SDKWORK_DEPLOYMENT_PROFILE
    ?? 'standalone';
}

export default defineConfig(({ mode }) => {
  const environment = resolveViteEnvironment(mode, process.env);
  const bootstrapPlugin = createSdkworkCredentialEntryBootstrapVitePlugin({
    environment,
  });
  return {
    plugins: [react(), ...(bootstrapPlugin ? [bootstrapPlugin] : [])],
    // One React instance, whichever repo a module is authored in — same dedupe
    // list `apps/sdkwork-deployments-pc/vite.config.ts` carries.
    resolve: {
      dedupe: ["react", "react-dom", "react-router", "react-router-dom"],
    },
    server: { port: 5182, strictPort: false },
    preview: { port: 4182 },
    build: {
      outDir: resolveBrowserDistOutDir(
        environment,
        resolveViteDeploymentProfile(mode, process.env),
      ),
      target: "es2022",
      sourcemap: true,
    },
  };
});
