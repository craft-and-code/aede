import { test } from "node:test";
import assert from "node:assert/strict";
import { devServerOptions, developmentArguments } from "../scripts/development.mjs";
test("desktop frontend selects its own free port without hiding explicit conflicts", () => {
  assert.deepEqual(devServerOptions({}), { host: "127.0.0.1", port: 1422, strictPort: false });
  assert.deepEqual(devServerOptions({ AEDE_DEV_PORT: "5555" }), {
    host: "127.0.0.1",
    port: 5555,
    strictPort: true,
  });
  for (const value of ["0", "bad", "65536"])
    assert.throws(() => devServerOptions({ AEDE_DEV_PORT: value }));
});
test("Tauri receives the actually bound URL before application arguments", () => {
  const args = developmentArguments(["dev", "--", "--example"], 1423);
  assert.equal(args[1], "--config");
  assert.deepEqual(JSON.parse(args[2]), {
    build: { devUrl: "http://127.0.0.1:1423", beforeDevCommand: "" },
  });
  assert.deepEqual(args.slice(3), ["--", "--example"]);
});
