import { spawn } from "node:child_process";
import { readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { componentExplorerArguments } from "./component-explorer-policy.mjs";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
if (process.argv.length !== 3) throw new Error("Specify exactly one mode: dev or build.");
const args = componentExplorerArguments(process.argv[2], root);
const manifestPath = fileURLToPath(import.meta.resolve("storybook/package.json"));
const manifest = JSON.parse(await readFile(manifestPath, "utf8"));
const bin = typeof manifest.bin === "string" ? manifest.bin : manifest.bin.storybook;
if (typeof bin !== "string") throw new Error("The installed Storybook CLI is unavailable.");
const child = spawn(process.execPath, [path.join(path.dirname(manifestPath), bin), ...args], {
  cwd: root,
  env: { ...process.env, STORYBOOK_DISABLE_TELEMETRY: "1" },
  stdio: "inherit",
});
const stop = (signal) => {
  if (!child.killed) child.kill(signal);
};
process.once("SIGINT", () => stop("SIGINT"));
process.once("SIGTERM", () => stop("SIGTERM"));
child.once("error", (error) => {
  console.error(error.message);
  process.exitCode = 1;
});
child.once("exit", (code, signal) => {
  process.exitCode = code ?? (signal ? 1 : 0);
});
