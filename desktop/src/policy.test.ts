import { describe, it, expect } from "vitest";
import { parsePort, taskActive, appendRoots } from "./policy";
describe("listener policy", () => {
  it("rejects zero, implicit fallback and malformed ports", () => {
    for (const value of ["0", "65536", "8787x", "-1", "", " 8787", "1.5"])
      expect(() => parsePort(value)).toThrow();
    expect(parsePort("8787")).toBe(8787);
    expect(parsePort("65535")).toBe(65535);
  });
  it("preserves folder order without registering exact duplicates", () =>
    expect(appendRoots(["/music/a"], ["/music/b", "/music/a"])).toEqual(["/music/a", "/music/b"]));
  it("polls pending tasks but preserves every terminal outcome", () => {
    for (const state of ["queued", "running", "cancel_requested"])
      expect(taskActive(state)).toBe(true);
    for (const state of ["completed", "failed", "cancelled", "unavailable", ""])
      expect(taskActive(state)).toBe(false);
  });
});
