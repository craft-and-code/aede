import { createRoot } from "solid-js";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type { Task } from "./platform";
import { createTaskController } from "./task-controller";

const cleanups: (() => void)[] = [];
beforeEach(() => vi.useFakeTimers());
afterEach(() => {
  for (const dispose of cleanups.splice(0)) dispose();
  vi.useRealTimers();
});

function setup() {
  const initial: Task = { task_id: 7, status: "running" };
  let resolve!: (task: Task) => void;
  const pending = new Promise<Task>((yes) => {
    resolve = yes;
  });
  const read = vi
    .fn<(id: number, cancel?: boolean) => Promise<Task>>()
    .mockReturnValueOnce(pending);
  const controller = createRoot((dispose) => {
    cleanups.push(dispose);
    return {
      ...createTaskController({ read, publishError: vi.fn(), publishMessage: vi.fn() }),
      dispose,
    };
  });
  controller.publish(initial);
  return { ...controller, read, resolve, initial };
}

it.each(["stop", "dispose", "replacement"])(
  "ignores a pending cancellation acknowledgement after %s retires its owner",
  async (action) => {
    const test = setup();
    const cancellation = test.cancel();
    expect(test.read.mock.calls).toEqual([[7, true]]);
    const replacement: Task = { task_id: 8, status: "running" };
    if (action === "stop") test.clear();
    else if (action === "dispose") test.dispose();
    else test.publish(replacement);
    test.resolve({ task_id: 7, status: "cancel_requested" });
    await cancellation;
    expect(test.task()).toEqual(
      action === "stop" ? undefined : action === "dispose" ? test.initial : replacement,
    );
  },
);

it.each(["completed", "failed", "cancelled"])(
  "preserves an observed %s outcome when an older cancellation acknowledgement arrives",
  async (status) => {
    const test = setup();
    const terminal: Task = { task_id: 7, status, result: { stdout: "final output\n" } };
    const cancellation = test.cancel();
    test.read.mockResolvedValueOnce(terminal);
    await vi.advanceTimersByTimeAsync(1000);
    expect(test.task()).toEqual(terminal);
    expect(test.active()).toBe(false);
    test.resolve({ task_id: 7, status: "cancel_requested" });
    await cancellation;
    expect(test.task()).toEqual(terminal);
    expect(test.active()).toBe(false);
    expect(vi.getTimerCount()).toBe(0);
    expect(test.read.mock.calls).toEqual([[7, true], [7]]);
  },
);
