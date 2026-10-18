import { createServer } from "vite";

/** A port is a development preference; an explicit choice must never silently change. */
export function devServerOptions(env = process.env) {
  const raw = env.AEDE_DEV_PORT;
  if (raw !== undefined && (!/^[1-9]\d*$/.test(raw) || Number(raw) > 65535))
    throw new Error("AEDE_DEV_PORT must be an integer from 1 to 65535.");
  return {
    host: "127.0.0.1",
    port: raw === undefined ? 1422 : Number(raw),
    strictPort: raw !== undefined,
  };
}

/** Inject the live URL before runner/app arguments; Tauri must not launch another Vite. */
export function developmentArguments(args, port) {
  if (args[0] !== "dev" || !Number.isInteger(port) || port < 1 || port > 65535)
    throw new Error("Invalid desktop development launch.");
  const split = args.indexOf("--"),
    at = split < 0 ? args.length : split;
  return [
    ...args.slice(0, at),
    "--config",
    JSON.stringify({ build: { devUrl: `http://127.0.0.1:${port}`, beforeDevCommand: "" } }),
    ...args.slice(at),
  ];
}

/** Bind Vite first, so a busy port cannot send the webview to another application's server. */
export async function startFrontend(config = {}) {
  const server = await createServer(config);
  try {
    await server.listen();
    const address = server.httpServer?.address();
    if (!address || typeof address === "string") throw new Error("Vite has no listening TCP port.");
    return { server, port: address.port };
  } catch (failure) {
    await server.close();
    throw failure;
  }
}
