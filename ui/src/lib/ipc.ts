import type { Action, ActionResult, AdbCheck, CommandLog, DetailItem, IconItem, LabelHandlers, NamesStatusInfo, ReportOut, StepResult, TuningInfo, DeviceInfo, HealthSample, PlanResponse, RestoreResponse, SnapshotMeta, AppInfo, Device, LabelItem, LabelProgress, MdnsService, QrStart, RunningResponse, SavedDevice } from "./types";
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
  deviceInfo: (serial: string): Promise<DeviceInfo> => (inTauri ? call("device_info", { serial }) : mock.deviceInfo(serial)),
  healthSample: (serial: string): Promise<HealthSample> => (inTauri ? call("health_sample", { serial }) : mock.healthSample(serial)),
  listRunning: (serial: string): Promise<RunningResponse> =>
    inTauri ? call("list_running", { serial }) : mock.listRunning(serial),
  /** Start finding names and icons; results arrive through the handlers. Calling it again retries. */
  loadLabels: async (serial: string, h: LabelHandlers) => {
    if (!inTauri) { mock.loadLabels(h); return () => {}; }
    const offs = [
      await listen<LabelItem[]>("labels", h.onLabels),
      await listen<LabelProgress>("labels-progress", h.onProgress),
      await listen<NamesStatusInfo>("names-status", h.onStatus),
      await listen<DetailItem[]>("bridge-details", h.onDetails),
      await listen<IconItem[]>("icons", h.onIcons),
    ];
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

  snapshots: {
    create: (serial: string, name: string, auto: boolean): Promise<SnapshotMeta> =>
      inTauri ? call("snapshot_create", { serial, name, auto }) : mock.snapshots.create(serial, name, auto),
    list: (): Promise<SnapshotMeta[]> => (inTauri ? call("snapshot_list") : mock.snapshots.list()),
    remove: (id: string): Promise<void> => (inTauri ? call("snapshot_delete", { id }) : mock.snapshots.remove(id)),
    plan: (serial: string, id: string): Promise<PlanResponse> =>
      inTauri ? call("snapshot_plan", { serial, id }) : mock.snapshots.plan(serial, id),
    restore: (serial: string, id: string): Promise<RestoreResponse> =>
      inTauri ? call("snapshot_restore", { serial, id }) : mock.snapshots.restore(serial, id),
    /** Asks where to save (native dialog in the app, a download in the browser). Resolves false if cancelled. */
    export: async (meta: SnapshotMeta): Promise<boolean> => {
      const file = `${meta.name.replace(/[^\w.-]+/g, "_") || "snapshot"}.json`;
      if (inTauri) {
        const { save } = await import("@tauri-apps/plugin-dialog");
        const path = await save({ defaultPath: file, filters: [{ name: "Snapshot", extensions: ["json"] }] });
        if (!path) return false;
        await call("snapshot_export", { id: meta.id, path });
        return true;
      }
      const url = URL.createObjectURL(new Blob([mock.snapshots.exportJson(meta.id)], { type: "application/json" }));
      Object.assign(document.createElement("a"), { href: url, download: file }).click();
      URL.revokeObjectURL(url);
      return true;
    },
    /** Asks for a snapshot file to import. Resolves null if cancelled. */
    import: async (): Promise<SnapshotMeta | null> => {
      if (inTauri) {
        const { open } = await import("@tauri-apps/plugin-dialog");
        const path = await open({ multiple: false, filters: [{ name: "Snapshot", extensions: ["json"] }] });
        if (!path || Array.isArray(path)) return null;
        return call("snapshot_import", { path });
      }
      const file = await new Promise<File | null>((res) => {
        const i = Object.assign(document.createElement("input"), { type: "file", accept: ".json" });
        i.onchange = () => res(i.files?.[0] ?? null);
        i.click();
      });
      return file ? mock.snapshots.importJson(await file.text()) : null;
    },
  },
  history: {
    load: (): Promise<unknown[]> => (inTauri ? call("history_load") : Promise.resolve(mock.history.load)),
    save: (entries: unknown[]): Promise<void> => (inTauri ? call("history_save", { entries }) : Promise.resolve()),
  },

  tuning: {
    status: (serial: string): Promise<TuningInfo> => (inTauri ? call("tuning_status", { serial }) : mock.tuning.status()),
    toggle: (serial: string, id: string, on: boolean): Promise<StepResult[]> =>
      inTauri ? call("tuning_toggle", { serial, id, on }) : mock.tuning.toggle(serial, id, on),
    restrict: (serial: string, names: string[], restrict: boolean): Promise<StepResult[]> =>
      inTauri ? call("tuning_restrict", { serial, names, restrict }) : mock.tuning.restrict(serial, names, restrict),
    undo: (serial: string, f: { group?: string; id?: string; package?: string }): Promise<StepResult[]> =>
      inTauri ? call("tuning_undo", { serial, group: f.group ?? null, id: f.id ?? null, package: f.package ?? null }) : mock.tuning.undo(serial, f),
    trimCaches: (serial: string): Promise<void> => (inTauri ? call("tuning_trim_caches", { serial }) : new Promise((r) => setTimeout(r, 600))),
    cooldown: (serial: string, apps: string[], stop: boolean): Promise<StepResult[]> =>
      inTauri ? call("tuning_cooldown", { serial, apps, stop }) : mock.tuning.cooldown(serial, apps, stop),
  },

  report: {
    collect: (serial: string): Promise<ReportOut> => (inTauri ? call("device_report", { serial }) : mock.report()),
    /** Asks where to save (native dialog in the app, a download in the browser). Resolves false if cancelled. */
    save: async (text: string): Promise<boolean> => {
      if (inTauri) {
        const { save } = await import("@tauri-apps/plugin-dialog");
        const path = await save({ defaultPath: "device-report.txt", filters: [{ name: "Text", extensions: ["txt"] }] });
        if (!path) return false;
        await call("save_text_file", { path, text });
        return true;
      }
      const url = URL.createObjectURL(new Blob([text], { type: "text/plain" }));
      Object.assign(document.createElement("a"), { href: url, download: "device-report.txt" }).click();
      URL.revokeObjectURL(url);
      return true;
    },
  },

  log: {
    since: (seq: number): Promise<CommandLog> => (inTauri ? call("get_log", { since: seq }) : mock.log.since(seq)),
    clear: (): Promise<void> => (inTauri ? call("clear_log") : mock.log.clear()),
  },
};
