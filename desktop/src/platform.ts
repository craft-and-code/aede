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
  result?: { stdout?: string; stderr?: string; exit_code?: number } | null;
  error?: { code: string; message: string } | null;
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

/** Safe request summaries, bounded to the latest 500 entries by the native host. */
export interface LogEntry {
  id: number;
  timeMs: number;
  level: "info" | "warn" | "error";
  message: string;
}
/** Incremental cursor remains monotonic when logs are cleared. */
export interface LogSnapshot {
  entries: LogEntry[];
  nextId: number;
}
/** Explicit HTTPS health check, distinct from process liveness. */
export interface Diagnostics {
  checkedAt: number;
  running: boolean;
  apiResponding: boolean;
  apiVersion: number | null;
  latencyMs: number;
  catalogLoaded: boolean;
  library: {
    scanned_at: number;
    files: number;
    artists: number;
    releases: number;
    tracks: number;
  } | null;
  error: string | null;
}
export const hostLogs = (after = 0) => call<LogSnapshot>("host_logs", { after });
export const clearLogs = () => call<void>("host_clear_logs");
export const hostDiagnostics = () => call<Diagnostics>("host_diagnostics");
/** Only these fixed native-owned values can be copied; no clipboard-reading IPC is exposed. */
export const copyAddress = () => call<void>("host_copy_address");
export const copyPassword = () => call<void>("host_copy_password");

export const copyUsername = () => call<void>("host_copy_username");
/** Export sanitized native journals through a save dialog; the webview chooses no file path. */
export const exportLogs = () => call<string | null>("host_export_logs");
