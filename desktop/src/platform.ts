import { invoke, isTauri } from "@tauri-apps/api/core";
/** Public status never includes a password, session token or private key. */
export interface HostStatus {
  running: boolean;
  address: string | null;
  port: number;
  roots: { path: string; available: boolean }[];
  username: string;
  dataDirectory: string;
  version: string;
  engines: { aede: string; flaccompagnon: string };
}
/** Bounded task lifecycle from the existing Aède administrative API. */
export interface Task {
  task_id: number;
  status: string;
  result?: { stdout?: string; stderr?: string; exit_code?: number };
  error?: string;
}
/** One versioned IPC boundary; the webview has no direct filesystem or server access. */
async function call<T>(command: string, args: Record<string, unknown> = {}): Promise<T> {
  if (!isTauri()) throw new Error("Cette action nécessite l’application Aède.");
  try {
    return await invoke<T>(command, { version: 1, ...args });
  } catch (error) {
    throw new Error(typeof error === "string" ? error : "Opération native impossible.");
  }
}
export const native = isTauri;
export const hostStatus = () => call<HostStatus>("host_status");
export const hostStart = () => call<HostStatus>("host_start");
export const hostStop = () => call<HostStatus>("host_stop");
export const hostConfigure = (port: number, roots: string[]) =>
  call<HostStatus>("host_configure", { port, roots });
export const chooseFolders = () => call<string[]>("host_folders");
export const hostScan = () => call<Task>("host_scan");
export const hostTask = (id: number, cancel = false) => call<Task>("host_task", { id, cancel });
export const hostPassword = () => call<string>("host_password");
export const exportConnection = () => call<string | null>("host_export");
