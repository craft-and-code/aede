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
  const read = vi.fn<(id: number, cancel?: boolean) => Promise<Task>>().mockResolvedValue(initial);
  const publishError = vi.fn();
  const publishMessage = vi.fn();
  const controller = createRoot((dispose) => {
    cleanups.push(dispose);
    return {
      ...createTaskController({ read, publishError, publishMessage }),
      dispose,
    };
  });
  controller.publish(initial);
  return { ...controller, initial, read, publishError, publishMessage };
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: Error) => void;
  const promise = new Promise<T>((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
}

it("retains an admitted scan after a transient observation failure and reads its completion", async () => {
  const test = setup();
  const completed: Task = {
    task_id: 7,
    status: "completed",
    result: { stdout: "scan finished\n", stderr: "", exit_code: 0 },
  };
  test.read.mockRejectedValueOnce(new Error("temporary IPC failure")).mockResolvedValue(completed);
  await vi.advanceTimersByTimeAsync(1000);
  expect(test.publishError).toHaveBeenCalledWith("Error: temporary IPC failure");
  expect(test.task()).toEqual(test.initial);
  expect(test.active()).toBe(true);
  expect(test.publishMessage).not.toHaveBeenCalled();
  await vi.advanceTimersByTimeAsync(1000);
  expect(test.read.mock.calls).toEqual([[7], [7]]);
  expect(test.task()).toEqual(completed);
  expect(test.active()).toBe(false);
  expect(test.publishMessage).toHaveBeenCalledWith(
    "Scan terminé. Le catalogue est disponible pour Phémios.",
  );
  await vi.advanceTimersByTimeAsync(5000);
  expect(test.read).toHaveBeenCalledTimes(2);
});

it("keeps individual cancellation available while observation is uncertain", async () => {
  const test = setup();
  test.read
    .mockRejectedValueOnce(new Error("temporary IPC failure"))
    .mockResolvedValueOnce({ task_id: 7, status: "cancel_requested" })
    .mockResolvedValueOnce({ task_id: 7, status: "cancelled" });
  await vi.advanceTimersByTimeAsync(1000);
  expect(test.active()).toBe(true);
  await test.cancel();
  expect(test.read.mock.calls).toEqual([[7], [7, true]]);
  expect(test.task()?.status).toBe("cancel_requested");
  expect(test.active()).toBe(true);
  await vi.advanceTimersByTimeAsync(1000);
  expect(test.task()?.status).toBe("cancelled");
  expect(test.active()).toBe(false);
  expect(test.read.mock.calls).toEqual([[7], [7, true], [7]]);
});

it.each(["completed", "failed", "cancelled"])(
  "preserves a genuine %s outcome and its command output without polling it again",
  async (status) => {
    const test = setup();
    const terminal: Task = {
      task_id: 7,
      status,
      result: { stdout: "retained output\n", stderr: "diagnostic\n", exit_code: 1 },
    };
    test.read.mockResolvedValue(terminal);
    await vi.advanceTimersByTimeAsync(1000);
    expect(test.task()).toEqual(terminal);
    expect(test.active()).toBe(false);
    expect(test.publishMessage).toHaveBeenCalledTimes(1);
    expect(vi.getTimerCount()).toBe(0);
    await vi.advanceTimersByTimeAsync(5000);
    expect(test.read.mock.calls).toEqual([[7]]);
    expect(test.publishError).not.toHaveBeenCalled();
  },
);

it("tracks queued and cancel_requested tasks until a terminal cancellation arrives", async () => {
  const test = setup();
  test.publish({ task_id: 7, status: "queued" });
  test.read
    .mockResolvedValueOnce({ task_id: 7, status: "cancel_requested" })
    .mockResolvedValueOnce({ task_id: 7, status: "cancelled" });
  expect(test.active()).toBe(true);
  expect(test.read).not.toHaveBeenCalled();
  await vi.advanceTimersByTimeAsync(1000);
  expect(test.task()?.status).toBe("cancel_requested");
  expect(test.active()).toBe(true);
  expect(test.publishMessage).not.toHaveBeenCalled();
  await vi.advanceTimersByTimeAsync(1000);
  expect(test.task()?.status).toBe("cancelled");
  expect(test.active()).toBe(false);
  expect(test.read.mock.calls).toEqual([[7], [7]]);
});

it("does not overlap polls while the current native request is pending", async () => {
  const test = setup();
  const pending = deferred<Task>();
  test.read.mockReturnValueOnce(pending.promise);
  await vi.advanceTimersByTimeAsync(1000);
  await vi.advanceTimersByTimeAsync(5000);
  expect(test.read.mock.calls).toEqual([[7]]);
  pending.resolve({ task_id: 7, status: "running" });
  await vi.advanceTimersByTimeAsync(0);
  await vi.advanceTimersByTimeAsync(1000);
  expect(test.read.mock.calls).toEqual([[7], [7]]);
});

it.each(["stop", "dispose"])(
  "ignores a pending poll completion after explicit %s",
  async (action) => {
    const test = setup();
    const pending = deferred<Task>();
    test.read.mockReturnValueOnce(pending.promise);
    await vi.advanceTimersByTimeAsync(1000);
    if (action === "stop") test.clear();
    else test.dispose();
    pending.resolve({ task_id: 7, status: "completed" });
    await vi.advanceTimersByTimeAsync(5000);
    expect(test.task()).toEqual(action === "stop" ? undefined : test.initial);
    expect(test.read.mock.calls).toEqual([[7]]);
    expect(test.publishMessage).not.toHaveBeenCalled();
    expect(test.publishError).not.toHaveBeenCalled();
    expect(vi.getTimerCount()).toBe(0);
  },
);

it.each(["stop", "dispose"])(
  "ignores a pending poll rejection after explicit %s",
  async (action) => {
    const test = setup();
    const pending = deferred<Task>();
    test.read.mockReturnValueOnce(pending.promise);
    await vi.advanceTimersByTimeAsync(1000);
    if (action === "stop") test.clear();
    else test.dispose();
    pending.reject(new Error("late rejection"));
    await vi.advanceTimersByTimeAsync(5000);
    expect(test.task()).toEqual(action === "stop" ? undefined : test.initial);
    expect(test.read.mock.calls).toEqual([[7]]);
    expect(test.publishMessage).not.toHaveBeenCalled();
    expect(test.publishError).not.toHaveBeenCalled();
    expect(vi.getTimerCount()).toBe(0);
  },
);

it.each(["completion", "rejection"])(
  "ignores a previous task's pending %s after tracking another admitted task",
  async (outcome) => {
    const test = setup();
    const pending = deferred<Task>();
    const replacement: Task = { task_id: 8, status: "running" };
    test.read.mockReturnValueOnce(pending.promise).mockResolvedValue(replacement);
    await vi.advanceTimersByTimeAsync(1000);
    test.publish(replacement);
    await vi.advanceTimersByTimeAsync(1000);
    if (outcome === "completion") pending.resolve({ task_id: 7, status: "completed" });
    else pending.reject(new Error("previous task rejection"));
    await vi.advanceTimersByTimeAsync(0);
    expect(test.task()).toEqual(replacement);
    expect(test.active()).toBe(true);
    expect(test.read.mock.calls).toEqual([[7], [8]]);
    expect(test.publishMessage).not.toHaveBeenCalled();
    expect(test.publishError).not.toHaveBeenCalled();
  },
);

it("does not restore an older running reply over an acknowledged cancellation request", async () => {
  const test = setup();
  const pending = deferred<Task>();
  test.read
    .mockReturnValueOnce(pending.promise)
    .mockResolvedValueOnce({ task_id: 7, status: "cancel_requested" })
    .mockResolvedValueOnce({ task_id: 7, status: "cancelled" });
  await vi.advanceTimersByTimeAsync(1000);
  await test.cancel();
  pending.resolve({ task_id: 7, status: "running" });
  await vi.advanceTimersByTimeAsync(0);
  expect(test.task()?.status).toBe("cancel_requested");
  expect(test.active()).toBe(true);
  await vi.advanceTimersByTimeAsync(1000);
  expect(test.task()?.status).toBe("cancelled");
  expect(test.read.mock.calls).toEqual([[7], [7, true], [7]]);
});
