import { spawnSync } from "node:child_process";
import { mkdir, readdir, readFile, writeFile, rm } from "node:fs/promises";
import path from "node:path";
// TypeScript 7 exposes its native CLI, not the earlier JavaScript compiler API.
// Declaration emission preserves public types and authored comments without a new dependency.
const root = process.cwd(),
  output = path.join(root, "dist-docs/frontend"),
  types = path.join(output, "types");
await rm(types, { recursive: true, force: true });
const compile = spawnSync(
  process.execPath,
  [
    path.join(root, "node_modules/typescript/bin/tsc"),
    "--noEmit",
    "false",
    "--declaration",
    "--emitDeclarationOnly",
    "--removeComments",
    "false",
    "--outDir",
    types,
  ],
  { stdio: "inherit" },
);
if (compile.error) throw compile.error;
if (compile.status !== 0) throw new Error("Frontend declaration documentation failed.");
const escape = (value) =>
  String(value)
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");
const modules = [];
for (const name of (await readdir(types, { recursive: true })).sort()) {
  if (
    !name.endsWith(".d.ts") ||
    name.endsWith(".test.d.ts") ||
    name.endsWith("test-fixtures.d.ts") ||
    name.includes("/node_modules/")
  )
    continue;
  const source = await readFile(path.join(types, name), "utf8");
  if (source.trim() === "export {};") continue;
  modules.push({ name, source });
}
if (!modules.length) throw new Error("No frontend declaration modules found.");
await mkdir(output, { recursive: true });
await writeFile(
  path.join(output, "index.html"),
  `<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Aède frontend contracts</title><style>body{font:16px/1.5 system-ui;background:#111213;color:#f1f0eb;margin:0;display:grid;grid-template-columns:270px 1fr}nav{padding:24px;border-right:1px solid #414446;overflow-wrap:anywhere}nav a{display:block;padding:5px;color:#d8f36a;text-decoration:none}main{padding:32px;min-width:0;max-width:1100px}h2{color:#d8f36a;border-bottom:1px solid #414446;padding-top:24px}pre{padding:16px;background:#1a1c1d;overflow:auto;border-radius:8px}@media(max-width:800px){body{display:block}nav{border:0}}</style><nav><h1>Aède</h1>${modules.map((module) => `<a href="#${encodeURIComponent(module.name)}">${escape(module.name)}</a>`).join("")}</nav><main><h1>Frontend public contracts</h1><p>Generated public TypeScript declarations and authored JSDoc/TSDoc-style comments. This is not formal TSDoc validation or a test-coverage report. Production implementations and tests are omitted.</p>${modules.map((module) => `<section id="${encodeURIComponent(module.name)}"><h2>${escape(module.name)}</h2><pre><code>${escape(module.source)}</code></pre></section>`).join("")}</main></html>`,
);
console.log(
  `Frontend documentation: ${modules.length} declaration modules → dist-docs/frontend/index.html`,
);
