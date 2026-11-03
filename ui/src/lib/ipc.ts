import type { Action, ActionResult, AdbCheck, AppInfo, Device, LabelItem, LabelProgress, MdnsService, QrStart, RunningResponse, SavedDevice } from "./types";
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

  wireless: {
    check: (): Promise<AdbCheck> => (inTauri ? call("adb_check") : mock.wireless.check()),
    discover: (): Promise<MdnsService[]> => (inTauri ? call("wireless_discover") : mock.wireless.discover()),
    pair: (address: string, code: string): Promise<string> =>
      inTauri ? call("wireless_pair", { address, code }) : mock.wireless.pair(address, code),
    connect: (address: string): Promise<string> =>
      inTauri ? call("wireless_connect", { address }) : mock.wireless.connect(address),
    disconnect: (address: string): Promise<void> =>
      inTauri ? call("wireless_disconnect", { address }) : mock.wireless.disconnect(address),
    usbToWireless: (serial: string): Promise<string> =>
      inTauri ? call("wireless_usb_to_wireless", { serial }) : mock.wireless.usbToWireless(serial),
    qrStart: (): Promise<QrStart> => (inTauri ? call("wireless_qr_start") : mock.wireless.qrStart()),
    qrWait: (q: QrStart): Promise<string> =>
      inTauri ? call("wireless_qr_wait", { name: q.name, password: q.password, id: q.id }) : mock.wireless.qrWait(),
    qrCancel: (): Promise<void> => (inTauri ? call("wireless_qr_cancel") : mock.wireless.qrCancel()),
    saved: (): Promise<SavedDevice[]> => (inTauri ? call("saved_devices") : mock.wireless.saved()),
    forget: (address: string): Promise<void> => (inTauri ? call("forget_device", { address }) : mock.wireless.forget(address)),
    reconnect: (address: string): Promise<string> =>
      inTauri ? call("wireless_reconnect", { address }) : mock.wireless.reconnect(address),
  },
};
