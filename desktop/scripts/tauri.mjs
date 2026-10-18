import { spawn } from "node:child_process";
import { createRequire } from "node:module";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const require = createRequire(import.meta.url);
const cli = join(dirname(require.resolve("@tauri-apps/cli/package.json")), "tauri.js");
const root = fileURLToPath(new URL("../", import.meta.url));
let frontend;
try {
  let args = process.argv.slice(2);
  if (args[0] === "dev") {
    await import("./prepare-engine.mjs");
    const { startFrontend, developmentArguments } = await import("./development.mjs");
    frontend = await startFrontend({ root });
    console.log(`[Aède] Development frontend: http://127.0.0.1:${frontend.port}`);
    args = developmentArguments(args, frontend.port);
  }
  const child = spawn(process.execPath, [cli, ...args], { stdio: "inherit", cwd: root });
  const interrupt = () => child.kill("SIGINT"),
    terminate = () => child.kill("SIGTERM");
  process.on("SIGINT", interrupt);
  process.on("SIGTERM", terminate);
  try {
    process.exitCode = await new Promise((resolve, reject) => {
      child.once("error", reject);
      child.once("close", (code, signal) => resolve(code ?? (signal === "SIGINT" ? 130 : 1)));
    });
  } finally {
    process.off("SIGINT", interrupt);
    process.off("SIGTERM", terminate);
  }
} catch (failure) {
  console.error(failure instanceof Error ? failure.message : String(failure));
  process.exitCode = 1;
} finally {
  await frontend?.server.close();
}
