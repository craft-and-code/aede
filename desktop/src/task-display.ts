import type { Task } from "./platform";

/** Plain text inserted into the task panel through Solid's text interpolation. */
export function taskOutput(task: Task | undefined): string | undefined {
  const result = task?.result;
  const output = result ? `${result.stdout ?? ""}${result.stderr ?? ""}` : undefined;
  const failure = task?.error?.message;
  if (!failure) return output;
  return output ? `${output}${output.endsWith("\n") ? "" : "\n"}${failure}` : failure;
}
