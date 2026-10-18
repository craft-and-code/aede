import { spawnSync } from "node:child_process";
import { mkdirSync, copyFileSync, chmodSync, writeFileSync, readFileSync } from "node:fs";
import { resolve, join } from "node:path";
const root = resolve(import.meta.dirname, "..");
function run(command, args, cwd = root) {
  const result = spawnSync(command, args, {
    cwd,
    encoding: "utf8",
    stdio: ["ignore", "pipe", "inherit"],
  });
  if (result.error || result.status !== 0)
    throw new Error(`${command} failed while preparing Aède.`);
  return result.stdout.trim();
}
const target =
  process.env.AEDE_DESKTOP_TARGET ??
  run("rustc", ["-vV"])
    .split("\n")
    .find((line) => line.startsWith("host: "))
    ?.slice(6);
if (!target || !/^[a-zA-Z0-9_-]+$/.test(target)) throw new Error("Invalid Rust target.");
const extension = target.includes("windows") ? ".exe" : "";
let binary = process.env.AEDE_DESKTOP_BINARY;
if (!binary) {
  const source = resolve(root, "..");
  run("cargo", ["build", "--release", "--locked", "--offline", "-p", "aede-cli"], source);
  binary = join(source, "target", "release", `aede${extension}`);
}
const versions = JSON.parse(run(binary, ["--version", "--json"]));
const app = JSON.parse(readFileSync(join(root, "package.json"), "utf8"));
if (
  versions.aede !== app.version ||
  typeof versions.flaccompagnon !== "string" ||
  !/^\d+\.\d+\.\d+$/.test(versions.flaccompagnon)
)
  throw new Error("App and embedded CLI versions must agree.");
const destination = join(root, "src-tauri", "binaries");
mkdirSync(destination, { recursive: true });
const output = join(destination, `aede-engine-${target}${extension}`);
copyFileSync(binary, output);
if (process.platform !== "win32") chmodSync(output, 0o755);
writeFileSync(join(destination, "versions.json"), JSON.stringify(versions));
console.log(
  `[Aède] Verified embedded CLI ${versions.aede} / FlacCompagnon ${versions.flaccompagnon}.`,
);
