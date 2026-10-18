import path from "node:path";

/** The explorer serves only loopback and publishes separately from the desktop frontend. */
export function componentExplorerArguments(mode, root) {
  const common = [
    "--config-dir",
    path.join(root, "docs/components/.storybook"),
    "--disable-telemetry",
  ];
  if (mode === "dev")
    return [
      "dev",
      ...common,
      "--host",
      "127.0.0.1",
      "--port",
      "6007",
      "--exact-port",
      "--ci",
      "--no-open",
      "--no-version-updates",
    ];
  if (mode === "build")
    return ["build", ...common, "--output-dir", path.join(root, "dist-docs/components")];
  throw new Error("Use component-explorer.mjs dev or build; extra CLI arguments are not accepted.");
}

/** Real presentation components may import pure policies, but never native or account providers. */
export function assertComponentExplorerImport(source, importer, root) {
  if (source.startsWith("@tauri-apps/") || source.startsWith("tauri:"))
    throw new Error("The component explorer cannot import Tauri capabilities.");
  const clean = source.split("?")[0];
  const resolved = path.isAbsolute(clean)
    ? path.normalize(clean)
    : clean.startsWith(".") && importer
      ? path.resolve(path.dirname(importer), clean)
      : undefined;
  if (!resolved) return;
  const local = path.relative(root, resolved).replaceAll(path.sep, "/");
  if (
    local.startsWith("src/platform/") ||
    /^src\/platform(?:\.[cm]?[jt]sx?)?$/.test(local) ||
    /^src\/library\/context(?:\.[cm]?[jt]sx?)?$/.test(local) ||
    /^src\/(?:main|App)(?:\.[cm]?[jt]sx?)?$/.test(local)
  )
    throw new Error(`The component explorer cannot import native application state: ${local}`);
}
