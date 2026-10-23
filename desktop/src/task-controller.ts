import { createEffect, createSignal, onCleanup } from "solid-js";
import type { Task } from "./platform";
import { taskActive } from "./policy";

/** Track the admitted task through the existing native task boundary. */
export function createTaskController(options: {
  read: (id: number, cancel?: boolean) => Promise<Task>;
  publishError: (message: string) => void;
  publishMessage: (message: string) => void;
}) {
  const [task, setTask] = createSignal<Task>();
  const active = () => taskActive(task()?.status ?? "");
  let alive = true;
  let generation = 0;
  const publish = (next: Task | undefined) => {
    generation++;
    return setTask(next);
  };
  onCleanup(() => {
    alive = false;
  });
  createEffect(() => {
    const id = task()?.task_id;
    if (!id || !active()) return;
    let cancelled = false,
      pending = false;
    const timer = setInterval(async () => {
      if (pending) return;
      pending = true;
      try {
        const next = await options.read(id);
        if (!cancelled && alive) {
          setTask(next);
          if (!taskActive(next.status))
            options.publishMessage(
              next.status === "completed"
                ? "Scan terminé. Le catalogue est disponible pour Phémios."
                : "La tâche s’est arrêtée. Consultez son résultat ci-dessous.",
            );
        }
      } catch (reason) {
        if (!cancelled && alive) {
          // An observation failure does not retire the admitted server task.
          options.publishError(String(reason));
        }
      } finally {
        pending = false;
      }
    }, 1000);
    onCleanup(() => {
      cancelled = true;
      clearInterval(timer);
    });
  });
  return {
    task,
    active,
    publish,
    clear: () => publish(undefined),
    cancel: async () => {
      const current = task();
      const expected = generation;
      if (!alive || !current || !active()) return;
      const next = await options.read(current.task_id, true);
      // Retiring or replacing the owner also retires its pending cancellation reply.
      if (alive && expected === generation && task()?.task_id === current.task_id && active())
        setTask(next);
    },
  };
}
