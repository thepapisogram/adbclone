import type { RatingsOverview, ImportOut, WakeReport, StorageReport, UsageReport, ExportResult, InspectOut, InstallOptions, Protection, Action, ActionResult, AdbCheck, AdbDownloadEvent, AdbStatus, CommandLog, DetailItem, IconItem, LabelHandlers, NamesStatusInfo, ReportOut, StepResult, TuningInfo, DeviceInfo, HealthSample, PlanResponse, RestoreResponse, SnapshotMeta, AppInfo, Device, LabelItem, LabelProgress, MdnsService, QrStart, RunningResponse, SavedDevice } from "./types";
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
  protections: (serial: string): Promise<Protection[]> => (inTauri ? call("get_protections", { serial }) : mock.protections(serial)),
  storage: (serial: string): Promise<StorageReport> => (inTauri ? call("storage_report", { serial }) : mock.storage(serial)),
  wake: (serial: string): Promise<WakeReport> => (inTauri ? call("wake_report", { serial }) : mock.wake(serial)),
  usage: (serial: string): Promise<UsageReport> => (inTauri ? call("usage_report", { serial }) : mock.usage(serial)),
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

  /** Asks where to save, then saves a screenshot of the phone. Resolves the path, or null if cancelled. */
  screenshot: async (serial: string): Promise<string | null> => {
    if (inTauri) {
      const { save } = await import("@tauri-apps/plugin-dialog");
      const path = await save({ defaultPath: `screenshot-${new Date().toISOString().slice(0, 19).replace(/[:T]/g, "-")}.png`, filters: [{ name: "Image", extensions: ["png"] }] });
      if (!path) return null;
      await call("screenshot_save", { serial, path });
      return path;
    }
    return mock.screenshot(serial);
  },
  mirrorAvailable: (): Promise<boolean> => (inTauri ? call("mirror_available") : mock.mirrorAvailable()),
  mirror: (serial: string): Promise<void> => (inTauri ? call("mirror_start", { serial }) : mock.mirror(serial)),
  ratings: {
    overview: (): Promise<RatingsOverview> => (inTauri ? call("ratings_overview") : mock.ratings.overview()),
    /** Asks for a ratings file. Resolves null if cancelled. */
    import: async (): Promise<ImportOut | null> => {
      if (inTauri) {
        const { open } = await import("@tauri-apps/plugin-dialog");
        const path = await open({ multiple: false, filters: [{ name: "Ratings", extensions: ["json"] }] });
        if (!path || Array.isArray(path)) return null;
        return call("ratings_import", { path });
      }
      const file = await new Promise<File | null>((res) => {
        const i = Object.assign(document.createElement("input"), { type: "file", accept: ".json" });
        i.onchange = () => res(i.files?.[0] ?? null);
        i.click();
      });
      return file ? mock.ratings.importJson(file.name, await file.text()) : null;
    },
    removeCommunity: (file: string): Promise<void> => (inTauri ? call("ratings_community_remove", { file }) : mock.ratings.removeCommunity(file)),
    setUser: (pkg: string, risk: string, desc: string): Promise<void> => (inTauri ? call("ratings_user_set", { package: pkg, risk, desc }) : mock.ratings.setUser(pkg, risk, desc)),
    removeUser: (pkg: string): Promise<void> => (inTauri ? call("ratings_user_remove", { package: pkg }) : mock.ratings.removeUser(pkg)),
    /** Saves your own ratings as a file to share. Resolves the count, or null if cancelled. */
    exportUser: async (): Promise<number | null> => {
      if (inTauri) {
        const { save } = await import("@tauri-apps/plugin-dialog");
        const path = await save({ defaultPath: "my-coolant-ratings.json", filters: [{ name: "Ratings", extensions: ["json"] }] });
        if (!path) return null;
        return call("ratings_user_export", { path });
      }
      const o = await mock.ratings.overview();
      if (!o.user.length) throw "You have not rated any apps yet.";
      const url = URL.createObjectURL(new Blob([JSON.stringify(o.user)], { type: "application/json" }));
      Object.assign(document.createElement("a"), { href: url, download: "my-coolant-ratings.json" }).click();
      URL.revokeObjectURL(url);
      return o.user.length;
    },
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
    animations: (serial: string, window: string, transition: string, animator: string): Promise<StepResult[]> =>
      inTauri ? call("tuning_animations", { serial, window, transition, animator }) : mock.tuning.animations(serial, window, transition, animator),
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

  adb: {
    status: (): Promise<AdbStatus> => (inTauri ? call("adb_status") : mock.adb.status()),
    /** Starts the download; progress and the result arrive through `on`. Returns a function that stops listening. */
    download: async (on: (e: AdbDownloadEvent) => void): Promise<() => void> => {
      if (!inTauri) { mock.adb.download(on); return () => {}; }
      const off = await listen<AdbDownloadEvent>("adb-download", on);
      try { await call("adb_download"); } catch (e) { off(); throw e; }
      return off;
    },
  },

  install: {
    /** Ask for files (or a folder) with the system dialog. Resolves to nothing if cancelled. */
    pick: async (kind: "files" | "folder"): Promise<string[]> => {
      if (!inTauri) return mock.install.pick(kind);
      const { open } = await import("@tauri-apps/plugin-dialog");
      const r = await open(kind === "folder" ? { directory: true, multiple: false } : { multiple: true, filters: [{ name: "Android apps", extensions: ["apk", "apks", "xapk", "apkm"] }] });
      return r == null ? [] : Array.isArray(r) ? r : [r];
    },
    /** Files dropped onto the window. `hover` says whether something is being dragged over it. Returns a function that stops listening. */
    onDrop: async (cb: (paths: string[]) => void, hover: (over: boolean) => void): Promise<() => void> => {
      if (inTauri) {
        const { getCurrentWebview } = await import("@tauri-apps/api/webview");
        return getCurrentWebview().onDragDropEvent((e) => {
          const p = e.payload;
          if (p.type === "drop") { hover(false); cb(p.paths); } else if (p.type === "enter" || p.type === "over") hover(true); else hover(false);
        });
      }
      const over = (e: DragEvent) => { e.preventDefault(); hover(true); };
      const leave = () => hover(false);
      const drop = (e: DragEvent) => { e.preventDefault(); hover(false); cb([...(e.dataTransfer?.files ?? [])].map((f) => `C:/Downloads/${f.name}`)); };
      window.addEventListener("dragover", over); window.addEventListener("dragleave", leave); window.addEventListener("drop", drop);
      return () => { window.removeEventListener("dragover", over); window.removeEventListener("dragleave", leave); window.removeEventListener("drop", drop); };
    },
    inspect: (paths: string[]): Promise<InspectOut[]> => (inTauri ? call("install_inspect", { paths }) : mock.install.inspect(paths)),
    run: (serial: string, id: string, o: InstallOptions): Promise<string> =>
      inTauri ? call("install_item", { serial, id, replace: o.replace, allowDowngrade: o.allowDowngrade, allowTest: o.allowTest }) : mock.install.run(serial, id, o),
    discard: (ids: string[]): Promise<void> => (inTauri ? call("install_discard", { ids }) : mock.install.discard(ids)),
  },
  backup: {
    chooseFolder: async (): Promise<string | null> => {
      if (!inTauri) return "C:/Backups";
      const { open } = await import("@tauri-apps/plugin-dialog");
      const r = await open({ directory: true, multiple: false });
      return typeof r === "string" ? r : null;
    },
    one: (serial: string, pkg: string, dest: string, meta: { label?: string; versionName?: string; installer?: string }): Promise<ExportResult> =>
      inTauri ? call("backup_one", { serial, package: pkg, dest, label: meta.label ?? null, versionName: meta.versionName ?? null, installer: meta.installer ?? null }) : mock.backup.one(serial, pkg, dest),
  },
};
