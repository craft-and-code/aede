import type { HostStatus, Task } from "./platform";

/** Selected folders take effect through the same bounded scan used by the CLI.
 * Publishing the running state before scan admission preserves access to Stop on failure.
 */
export function createLibraryActions(options: {
  status: () => HostStatus | undefined;
  configure: (port: number, roots: string[]) => Promise<HostStatus>;
  start: () => Promise<HostStatus>;
  scan: () => Promise<Task>;
  publish: (status: HostStatus) => void;
  publishTask: (task: Task) => void;
}) {
  async function scanSelected(status: HostStatus) {
    // Empty selections retain the existing catalogue; erasure is not a side effect of Start.
    if (status.running && status.roots.length) options.publishTask(await options.scan());
  }
  return {
    async start(port: number, roots: string[]) {
      options.publish(await options.configure(port, roots));
      const status = await options.start();
      options.publish(status);
      await scanSelected(status);
    },
    async configure(port: number, roots: string[]) {
      const previous = options.status()?.roots.map((row) => row.path) ?? [];
      const status = await options.configure(port, roots);
      options.publish(status);
      if (previous.length !== roots.length || roots.some((path) => !previous.includes(path)))
        await scanSelected(status);
    },
  };
}
