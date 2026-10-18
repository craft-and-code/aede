import path from "node:path";
import type { StorybookConfig } from "storybook-solidjs-vite";
const config: StorybookConfig = {
  stories: ["../stories/**/*.stories.tsx"],
  framework: {
    name: "storybook-solidjs-vite",
    options: {
      builder: { viteConfigPath: path.resolve(import.meta.dirname, "../vite.config.ts") },
    },
  },
  addons: ["@storybook/addon-docs"],
  core: { disableTelemetry: true },
};
export default config;
