import path from "node:path";
import { defineConfig } from "vite";
import { assertComponentExplorerImport } from "../../scripts/component-explorer-policy.mjs";
const root = path.resolve(import.meta.dirname, "../..");
export default defineConfig({
  publicDir: false,
  envDir: false,
  envPrefix: [],
  plugins: [
    {
      name: "aede-component-boundary",
      enforce: "pre",
      resolveId(source, importer) {
        assertComponentExplorerImport(source, importer, root);
        return null;
      },
    },
  ],
  server: {
    host: "127.0.0.1",
    fs: {
      strict: true,
      allow: [root],
      deny: [".env", ".env.*", "**/.git/**", "**/src-tauri/**", "**/core/**", "**/*.pem"],
    },
  },
});
