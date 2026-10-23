import { expect, it } from "vitest";
import type { Task } from "./platform";
import { taskOutput } from "./task-display";

// Exercise the nullable object schema received through native IPC, including before
// the frontend contract is corrected to describe the server's actual failure shape.
function wireTask(value: unknown): Task {
  return JSON.parse(JSON.stringify(value));
}

it("displays the backend failure reason literally when no command result exists", () => {
  const reason = 'cannot start administrative command: <volume> & "permission denied"';
  const task = wireTask({
    task_id: 7,
    status: "failed",
    result: null,
    error: { code: "command_failed", message: reason },
  });
  expect(taskOutput(task)).toBe(reason);
});

it.each(["completed", "failed", "cancelled"])(
  "preserves ordinary stdout and stderr for %s commands",
  (status) => {
    const task: Task = {
      task_id: 7,
      status,
      result: { stdout: "retained scan progress\n", stderr: "volume diagnostic\n", exit_code: 1 },
    };
    expect(taskOutput(task)).toBe("retained scan progress\nvolume diagnostic\n");
  },
);

it("keeps captured output alongside the backend cancellation explanation", () => {
  const task = wireTask({
    task_id: 7,
    status: "cancelled",
    result: { stdout: "published partial scan\n", stderr: "scan stopped\n", exit_code: 130 },
    error: { code: "task_cancelled", message: "task cancelled by user" },
  });
  const output = taskOutput(task);
  expect(output).toContain("published partial scan\nscan stopped\n");
  expect(output).toContain("task cancelled by user");
});

it("passes markup-like failure content as plain text for the task panel's text interpolation", () => {
  const reason = '<script>alert("scan")</script> & <img src=x onerror=alert(1)>';
  const task = wireTask({
    task_id: 7,
    status: "failed",
    result: null,
    error: { code: "command_failed", message: reason },
  });
  expect(taskOutput(task)).toBe(reason);
});

it("retains empty captured output and leaves tasks without any output empty", () => {
  expect(taskOutput({ task_id: 7, status: "completed", result: { stdout: "", stderr: "" } })).toBe(
    "",
  );
  expect(taskOutput(wireTask({ task_id: 7, status: "queued", result: null, error: null }))).toBe(
    undefined,
  );
  expect(taskOutput(undefined)).toBe(undefined);
});
