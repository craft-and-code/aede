import { expect, it, vi } from "vitest";
import { createLibraryActions } from "./library-actions";
import type { HostStatus } from "./platform";

function setup(running = false, paths = ["/music/first"]) {
  let status = { running, roots: paths.map((path) => ({ path, available: true })) } as HostStatus;
  const order: string[] = [];
  const configure = vi.fn(async (_port: number, roots: string[]) => {
    order.push("configure");
    return { ...status, roots: roots.map((path) => ({ path, available: true })) };
  });
  const start = vi.fn(async () => {
    order.push("start");
    return { ...status, running: true };
  });
  const scan = vi.fn(async () => {
    order.push("scan");
    return { task_id: 7, status: "queued" };
  });
  const publishTask = vi.fn();
  const actions = createLibraryActions({
    status: () => status,
    configure,
    start,
    scan,
    publish: (value) => (status = value),
    publishTask,
  });
  return { actions, configure, start, scan, publishTask, order, status: () => status };
}

it("starts the server then scans its selected roots and exposes the task", async () => {
  const test = setup();
  await test.actions.start(8787, ["/music/new"]);
  expect(test.order).toEqual(["configure", "start", "scan"]);
  expect(test.status().roots[0]?.path).toBe("/music/new");
  expect(test.publishTask).toHaveBeenCalledWith({ task_id: 7, status: "queued" });
});

it("scans changed roots on a running server without restarting it", async () => {
  const test = setup(true);
  await test.actions.configure(8787, ["/music/new"]);
  expect(test.order).toEqual(["configure", "scan"]);
  expect(test.start).not.toHaveBeenCalled();
});

it("retains folder edits offline until start and avoids scanning unchanged selections", async () => {
  const offline = setup();
  await offline.actions.configure(8787, ["/music/new"]);
  expect(offline.scan).not.toHaveBeenCalled();
  const online = setup(true, ["/music/first", "/music/second"]);
  await online.actions.configure(8787, ["/music/second", "/music/first"]);
  expect(online.scan).not.toHaveBeenCalled();
});

it("does not implicitly empty a catalogue when no folders are selected", async () => {
  const test = setup(false, []);
  await test.actions.start(8787, []);
  expect(test.scan).not.toHaveBeenCalled();
});

it("reports scan admission failure while retaining the started server and folder configuration", async () => {
  const test = setup();
  test.scan.mockRejectedValueOnce(new Error("scan busy"));
  await expect(test.actions.start(8787, ["/music/new"])).rejects.toThrow("scan busy");
  expect(test.status().running).toBe(true);
  expect(test.status().roots[0]?.path).toBe("/music/new");
  expect(test.publishTask).not.toHaveBeenCalled();
});
