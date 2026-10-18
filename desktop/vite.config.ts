import { defineConfig } from "vite";
import solid from "vite-plugin-solid";
import { devServerOptions } from "./scripts/development.mjs";
export default defineConfig({
  plugins: [solid()],
  clearScreen: false,
  server: { ...devServerOptions(), watch: { ignored: ["**/target/**", "**/src-tauri/**"] } },
  build: { target: "es2022" },
});
