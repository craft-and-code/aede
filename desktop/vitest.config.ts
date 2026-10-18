import { defineConfig } from "vitest/config";

export default defineConfig({
  resolve: { alias: [{ find: /^solid-js$/, replacement: "solid-js/dist/solid.js" }] },
  test: { include: ["src/**/*.test.ts"], passWithNoTests: true },
});
