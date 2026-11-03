import type { Action, ActionResult, AppInfo, Device } from "./types";
import { mock } from "./mock";

const inTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<T>(cmd, args);
}

/** Real Rust backend inside Tauri, mock data in a plain browser. */
export const ipc = {
  isMock: !inTauri,
  listDevices: (): Promise<Device[]> => (inTauri ? call("list_devices") : mock.listDevices()),
  listApps: (serial: string): Promise<AppInfo[]> =>
    inTauri ? call("list_apps", { serial }) : mock.listApps(serial),
  applyAction: (serial: string, action: Action, names: string[]): Promise<ActionResult[]> =>
    inTauri ? call("apply_action", { serial, action, names }) : mock.applyAction(serial, action, names),
};
