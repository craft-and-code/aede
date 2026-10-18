import { describe, it, expect } from "vitest";
import { parsePort, taskActive, appendRoots, mergeLogs } from "./policy";
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

describe("incremental journals", () => {
  it("preserves existing row identities and caps entries without duplicating cursors", () => {
    const first = { id: 1, message: "first" };
    expect(
      mergeLogs(
        [first],
        [
          { id: 1, message: "duplicate" },
          { id: 2, message: "new" },
        ],
      )[0],
    ).toBe(first);
    const entries = mergeLogs(
      [],
      Array.from({ length: 505 }, (_, index) => ({ id: index + 1 })),
    );
    expect(entries).toHaveLength(500);
    expect(entries[0]!.id).toBe(6);
    expect(entries[499]!.id).toBe(505);
  });
});
