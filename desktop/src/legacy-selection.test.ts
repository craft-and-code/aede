import { expect, it, vi } from "vitest";
import { createLibraryActions } from "./library-actions";
import type { HostStatus } from "./platform";

it("publishes a saved legacy reduction before scan refusal so the next removal uses it", async () => {
  let status = {
    running: true,
    port: 8787,
    roots: Array.from({ length: 8 }, (_, index) => ({
      path: `/absent-volume-${index}`,
      available: false,
    })),
  } as HostStatus;
  const events: string[] = [];
  const configure = vi.fn(async (_port: number, roots: string[]) => {
    events.push(`saved:${roots.length}`);
    return { ...status, roots: roots.map((path) => ({ path, available: false })) };
  });
  const scan = vi.fn(async () => {
    events.push(`scan:${status.roots.length}`);
    throw new Error("Reduce selected folders before scanning");
  });
  const publishTask = vi.fn();
  const start = vi.fn();
  const actions = createLibraryActions({
    status: () => status,
    configure,
    start,
    scan,
    publish: (next) => {
      status = next;
      events.push(`visible:${status.roots.length}`);
    },
    publishTask,
  });
  for (const count of [7, 6]) {
    const removed = status.roots.at(-1)?.path;
    const roots = status.roots.map((row) => row.path).filter((path) => path !== removed);
    await expect(actions.configure(8787, roots)).rejects.toThrow(
      "Reduce selected folders before scanning",
    );
    expect(status.roots.map((row) => row.path)).toEqual(roots);
    expect(status.running).toBe(true);
    expect(status.roots).toHaveLength(count);
  }
  expect(events).toEqual(["saved:7", "visible:7", "scan:7", "saved:6", "visible:6", "scan:6"]);
  expect(configure.mock.calls[1]?.[1]).toEqual(status.roots.map((row) => row.path));
  expect(start).not.toHaveBeenCalled();
  expect(publishTask).not.toHaveBeenCalled();
});

it("keeps the visible roots and skips scan when native configuration refuses an edit", async () => {
  const status = {
    running: true,
    roots: [{ path: "/selected-volume", available: false }],
  } as HostStatus;
  const publish = vi.fn();
  const scan = vi.fn();
  const actions = createLibraryActions({
    status: () => status,
    configure: vi.fn().mockRejectedValue(new Error("Selection refused")),
    start: vi.fn(),
    scan,
    publish,
    publishTask: vi.fn(),
  });
  await expect(actions.configure(8787, ["/replacement-volume"])).rejects.toThrow(
    "Selection refused",
  );
  expect(publish).not.toHaveBeenCalled();
  expect(scan).not.toHaveBeenCalled();
  expect(status.roots[0]?.path).toBe("/selected-volume");
});
