import type { InlineConfig, ViteDevServer } from "vite";
export function devServerOptions(env?: Record<string, string | undefined>): {
  host: string;
  port: number;
  strictPort: boolean;
};
export function developmentArguments(args: string[], port: number): string[];
export function startFrontend(
  config?: InlineConfig,
): Promise<{ server: ViteDevServer; port: number }>;
