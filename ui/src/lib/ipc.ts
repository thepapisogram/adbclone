import type { Action, ActionResult, AppInfo, Device, LabelItem, LabelProgress, RunningResponse } from "./types";
import { mock } from "./mock";

const inTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<T>(cmd, args);
}

/** Real Rust backend inside Tauri, mock data in a plain browser. */
async function listen<T>(event: string, cb: (p: T) => void): Promise<() => void> {
  const { listen } = await import("@tauri-apps/api/event");
  return listen<T>(event, (e) => cb(e.payload));
}

export const ipc = {
  isMock: !inTauri,
  listDevices: (): Promise<Device[]> => (inTauri ? call("list_devices") : mock.listDevices()),
  listApps: (serial: string): Promise<AppInfo[]> =>
    inTauri ? call("list_apps", { serial }) : mock.listApps(serial),
  applyAction: (serial: string, action: Action, names: string[]): Promise<ActionResult[]> =>
    inTauri ? call("apply_action", { serial, action, names }) : mock.applyAction(serial, action, names),
  listRunning: (serial: string): Promise<RunningResponse> =>
    inTauri ? call("list_running", { serial }) : mock.listRunning(serial),
  /** Start resolving real names; results arrive through `onLabels` / `onLabelProgress`. */
  loadLabels: async (serial: string, onLabels: (l: LabelItem[]) => void, onProgress: (p: LabelProgress) => void) => {
    if (!inTauri) { mock.loadLabels(onLabels, onProgress); return () => {}; }
    const offs = [await listen<LabelItem[]>("labels", onLabels), await listen<LabelProgress>("labels-progress", onProgress)];
    await call("load_labels", { serial });
    return () => offs.forEach((f) => f());
  },
};
