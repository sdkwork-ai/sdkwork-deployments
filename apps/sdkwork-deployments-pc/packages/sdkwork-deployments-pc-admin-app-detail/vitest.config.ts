import path from "node:path";
import { fileURLToPath } from "node:url";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vitest/config";

const PACKAGE_ROOT = path.dirname(fileURLToPath(import.meta.url));

/**
 * Test project of the application-detail capability.
 *
 * React must resolve to exactly one copy. This app composes `@sdkwork/ui-pc-react`
 * and `@sdkwork/i18n-pc-react` from sibling repositories through workspace links,
 * and each linked package resolves `react` from its own checkout. Without the
 * aliases below, the framework component and the component that renders it land
 * on two React copies, and the shared internals come back null — reported as
 * "Cannot read properties of null (reading 'useMemo')" from whichever hook runs
 * first.
 *
 * `dedupe` alone is not enough for that shape: it only collapses copies visible to
 * this workspace, and the linked repositories have their own store. Naming this
 * package's own React explicitly is what pins every importer.
 */
export default defineConfig({
  plugins: [react()],
  resolve: {
    // Workspace SDKs are consumed as TypeScript sources through
    // `package.json#exports`; `.ts` must win over a stale `.js` artifact.
    extensions: [".mts", ".ts", ".tsx", ".mjs", ".js", ".jsx", ".json"],
    alias: [
      { find: /^react$/, replacement: path.join(PACKAGE_ROOT, "node_modules/react") },
      { find: /^react\/jsx-runtime$/, replacement: path.join(PACKAGE_ROOT, "node_modules/react/jsx-runtime") },
      { find: /^react\/jsx-dev-runtime$/, replacement: path.join(PACKAGE_ROOT, "node_modules/react/jsx-dev-runtime") },
      { find: /^react-dom$/, replacement: path.join(PACKAGE_ROOT, "node_modules/react-dom") },
      { find: /^react-dom\/client$/, replacement: path.join(PACKAGE_ROOT, "node_modules/react-dom/client") },
      { find: /^react-i18next$/, replacement: path.join(PACKAGE_ROOT, "node_modules/react-i18next") },
    ],
  },
  test: {
    environment: "jsdom",
    include: ["tests/**/*.test.{ts,tsx}"],
  },
});
