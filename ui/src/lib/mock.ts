import type { Action, ActionResult, AdbCheck, DeviceInfo, HealthSample, Change, PlanResponse, RestoreResponse, SnapshotMeta, AppInfo, Device, LabelItem, LabelProgress, MdnsService, QrStart, RunningResponse, Safety, SavedDevice } from "./types";

const apps: AppInfo[] = [
  ["com.google.android.youtube", "YouTube", true, "recommended", "Video app; reinstallable from Play Store."],
  ["com.google.android.apps.maps", "Maps", true, "recommended", "Google Maps."],
  ["com.google.android.gm", "Gmail", true, "advanced", "Email client."],
  ["com.google.android.gms", "Google Play services", true, "unsafe", "Core services; many apps break without it."],
  ["com.google.android.gsf", "Google Services Framework", true, "unsafe", "Required for Google account sync and push."],
  ["com.android.vending", "Google Play Store", true, "advanced", "App store."],
  ["com.android.systemui", "System UI", true, "unsafe", "Status bar, navigation and lock screen. Disabling bricks the UI."],
  ["com.android.settings", "Settings", true, "unsafe", "Device settings."],
  ["com.android.phone", "Phone Services", true, "unsafe", "Telephony stack."],
  ["com.android.chrome", "Chrome", true, "advanced", "Default browser."],
  ["com.facebook.appmanager", "Facebook App Manager", true, "recommended", "Preinstalled background updater."],
  ["com.facebook.services", "Facebook Services", true, "recommended", "Preinstalled background service."],
  ["com.facebook.system", "Facebook Installer", true, "recommended", "Preinstalled installer stub."],
  ["com.samsung.android.bixby.agent", "Bixby Voice", true, "recommended", "Voice assistant."],
  ["com.samsung.android.game.gamehome", "Game Launcher", true, "recommended", "Game launcher overlay."],
  ["com.microsoft.office.officehubrow", "Microsoft 365", true, "recommended", "Preinstalled Office hub."],
  ["com.netflix.partner.activation", "Netflix Activation", true, "recommended", "Partner activation stub."],
  ["com.google.android.apps.photos", "Photos", true, "advanced", "Gallery and backup."],
  ["com.android.stk", "SIM Toolkit", true, "advanced", "Carrier SIM menu."],
  ["com.vendor.unknownservice", "Vendor Service", true, "unknown", ""],
  ["com.sec.android.app.randomthing", "Random Thing", true, "unknown", ""],
  ["org.mozilla.firefox", "Firefox", false, "unknown", ""],
  ["com.spotify.music", "Spotify", false, "unknown", ""],
  ["org.telegram.messenger", "Telegram", false, "unknown", ""],
  ["com.whatsapp", "WhatsApp", false, "unknown", ""],
  ["com.discord", "Discord", false, "unknown", ""],
  ["com.instagram.android", "Instagram", false, "unknown", ""],
].map(([name, label, system, risk, description]) => ({
  name: name as string,
  label: label as string,
  disableSafety: ({ recommended: "safe", advanced: "caution", unsafe: "unsafe", unknown: "unknown" } as Record<string, Safety>)[risk as string],
  uninstallSafety: ({ recommended: "safe", advanced: "caution", unsafe: "unsafe", unknown: "unknown" } as Record<string, Safety>)[risk as string],
  system: system as boolean,
  state: "enabled" as const,
  risk: risk as AppInfo["risk"],
  description: description as string,
}));
apps.find((a) => a.name === "com.android.stk")!.state = "disabled";

const guess = (n: string) => { const l = n.split(".").pop() ?? n; return l.charAt(0).toUpperCase() + l.slice(1); };
const wifi = new Set<string>();
const savedList: SavedDevice[] = [];
let qrCancelled = false;
const fakeQr = '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 8 8" width="200" height="200" shape-rendering="crispEdges"><rect width="8" height="8" fill="#fff"/><path d="M0 0h3v3H0zM5 0h3v3H5zM0 5h3v3H0zM4 4h1v1H4zM6 5h2v1H6zM5 7h1v1H5z" fill="#000"/></svg>';
interface MockSnap extends SnapshotMeta { states: Record<string, AppInfo["state"]> }
const snaps: MockSnap[] = [];
const metaOf = (m: MockSnap): SnapshotMeta => { const { states: _s, ...rest } = m; return rest; };
function takeSnap(name: string, auto: boolean): MockSnap {
  const states = Object.fromEntries(apps.map((a) => [a.name, a.state]));
  const v = Object.values(states);
  const m: MockSnap = { id: `${Date.now()}-${snaps.length}`, name, created: Math.floor(Date.now() / 1000), auto, deviceModel: "Pixel 7 (mock)",
    enabled: v.filter((x) => x === "enabled").length, disabled: v.filter((x) => x === "disabled").length, uninstalled: v.filter((x) => x === "uninstalled").length, states };
  snaps.unshift(m);
  return m;
}
const stepsFor = (from: string, to: string): Change["steps"] =>
  from === "uninstalled" ? ["restore", to === "enabled" ? "enable" : "disable"] : to === "uninstalled" ? ["uninstall"] : [to === "enabled" ? "enable" : "disable"];
function planFor(m: MockSnap): PlanResponse["plan"] {
  const changes: Change[] = []; const missing: string[] = []; let unchanged = 0;
  for (const [name, to] of Object.entries(m.states)) {
    const a = apps.find((x) => x.name === name);
    if (!a) missing.push(name); else if (a.state === to) unchanged++; else changes.push({ name, from: a.state, to, steps: stepsFor(a.state, to) });
  }
  const rank = (c: Change) => (c.steps[0] === "restore" ? 0 : c.steps[0] === "enable" ? 1 : c.steps[0] === "disable" ? 2 : 3);
  changes.sort((x, y) => rank(x) - rank(y));
  return { changes, missing, unchanged };
}
const delay = (ms = 250) => new Promise((r) => setTimeout(r, ms));

export const mock = {
  async listDevices(): Promise<Device[]> {
    await delay(100);
    return [
      { serial: "R58M123ABC", state: "device", model: "Pixel 7 (mock)" },
      { serial: "192.168.1.30:5555", state: "unauthorized", model: null },
      ...[...wifi].map((serial) => ({ serial, state: "device", model: "Pixel 7 (mock, Wi-Fi)" })),
    ];
  },
  async listApps(_serial: string): Promise<AppInfo[]> {
    await delay();
    // Like the real backend, start with names guessed from the package id.
    return apps.map((a) => ({ ...a, label: guess(a.name) }));
  },
  /** Streams the real names in a few batches, like the Rust backend does. */
  loadLabels(onLabels: (l: LabelItem[]) => void, onProgress: (p: LabelProgress) => void) {
    const total = apps.length;
    const batches = 5;
    const size = Math.ceil(total / batches);
    for (let i = 0; i < batches; i++) {
      setTimeout(() => {
        const part = apps.slice(i * size, (i + 1) * size);
        onLabels(part.map((a) => ({ name: a.name, label: a.label })));
        onProgress({ done: Math.min(total, (i + 1) * size), total });
      }, 500 + i * 450);
    }
  },
  _tick: 0,
  async listRunning(_serial: string): Promise<RunningResponse> {
    await delay(150);
    this._tick++;
    const rows = apps
      .filter((a) => a.state === "enabled")
      .map((a, i) => {
        const base = 40_000 + ((i * 37_117) % 260_000);
        const wobble = 1 + Math.sin(this._tick / 3 + i) * 0.08;
        return {
          package: a.name,
          rssKb: Math.round(base * wobble),
          cpu: this._tick === 1 ? null : Math.max(0, Math.round((((i * 7) % 11) / 2 + Math.random() * 3) * 10) / 10),
          processes: 1 + (i % 3),
          risk: a.risk,
          disableSafety: a.disableSafety,
          uninstallSafety: a.uninstallSafety,
          description: a.description,
        };
      });
    rows.push({ package: null as unknown as string, rssKb: 610_000, cpu: this._tick === 1 ? null : 12.4, processes: 94,
      risk: "unknown", disableSafety: "unknown", uninstallSafety: "unknown", description: "Kernel threads and system daemons" });
    rows.sort((a, b) => b.rssKb - a.rssKb);
    return { rows, memTotalKb: 3_862_908, memAvailableKb: 1_450_000, cores: 8 };
  },
  async applyAction(_serial: string, action: Action, names: string[]): Promise<ActionResult[]> {
    await delay(400);
    return names.map((name) => {
      const a = apps.find((x) => x.name === name);
      if (!a) return { name, ok: false, message: "Package not found" };
      if (name === "com.android.systemui" && action !== "enable")
        return { name, ok: false, message: "Failure [DELETE_FAILED_DEVICE_POLICY_MANAGER]" };
      if (action === "stop") return { name, ok: true };
      a.state = action === "disable" ? "disabled" : action === "uninstall" ? "uninstalled" : "enabled";
      return { name, ok: true };
    });
  },

  wireless: {
    async check(): Promise<AdbCheck> { await delay(100); return { major: 35, ok: true, message: "adb 35" }; },
    async discover(): Promise<MdnsService[]> {
      await delay(600);
      return [
        { name: "adb-R58M123ABC-AbCdEf", kind: "pairing", address: "192.168.1.20:37099" },
        { name: "adb-R58M123ABC-AbCdEf", kind: "connect", address: "192.168.1.20:41231" },
      ];
    },
    async pair(address: string, code: string): Promise<string> {
      await delay(800);
      if (code !== "123456") throw "Wrong pairing code, or the code expired. Reopen \"Pair device with pairing code\" on the phone and try again. (mock: use 123456)";
      return `Successfully paired to ${address} [guid=adb-MOCK-xyz]`;
    },
    async connect(address: string): Promise<string> {
      await delay(700);
      if (address.endsWith(":1")) throw "Connection refused. Is Wireless debugging turned on? The port changes every time you toggle it: use the port shown on the phone (or the discovered one).";
      wifi.add(address);
      if (!savedList.some((d) => d.address === address)) savedList.unshift({ name: "Pixel 7 (mock)", address, mdnsName: "adb-R58M123ABC-AbCdEf" });
      return `connected to ${address}`;
    },
    async disconnect(address: string): Promise<void> { await delay(200); wifi.delete(address); },
    async usbToWireless(_serial: string): Promise<string> {
      await delay(1500);
      wifi.add("192.168.1.20:5555");
      return "192.168.1.20:5555";
    },
    async qrStart(): Promise<QrStart> { qrCancelled = false; return { svg: fakeQr, name: "adbclone-mock", password: "mockpass12", id: 1 }; },
    async qrWait(): Promise<string> {
      for (let i = 0; i < 12; i++) { await delay(500); if (qrCancelled) throw "Cancelled."; }
      wifi.add("192.168.1.20:41231");
      return "connected to 192.168.1.20:41231";
    },
    async qrCancel(): Promise<void> { qrCancelled = true; },
    async saved(): Promise<SavedDevice[]> { return savedList.map((d) => ({ ...d })); },
    async forget(address: string): Promise<void> { const i = savedList.findIndex((d) => d.address === address); if (i >= 0) savedList.splice(i, 1); },
    async reconnect(address: string): Promise<string> { await delay(600); wifi.add(address); return `connected to ${address}`; },
  },

  snapshots: {
    async create(_serial: string, name: string, auto: boolean): Promise<SnapshotMeta> { await delay(300); return metaOf(takeSnap(name || "Snapshot", auto)); },
    async list(): Promise<SnapshotMeta[]> { return snaps.map(metaOf); },
    async remove(id: string): Promise<void> { const i = snaps.findIndex((m) => m.id === id); if (i >= 0) snaps.splice(i, 1); },
    async plan(_serial: string, id: string): Promise<PlanResponse> {
      await delay(300);
      const m = snaps.find((x) => x.id === id)!;
      return { plan: planFor(m), snapshotName: m.name, snapshotModel: m.deviceModel, deviceModel: "Pixel 7 (mock)" };
    },
    async restore(_serial: string, id: string): Promise<RestoreResponse> {
      await delay(700);
      const m = snaps.find((x) => x.id === id)!;
      const undo = takeSnap(`Before restoring ${m.name}`, true);
      const plan = planFor(m);
      const results = plan.changes.map((c) => {
        if (c.name === "com.google.android.apps.photos" && c.to !== "enabled") return { name: c.name, ok: false, message: "disable: Failure [DELETE_FAILED_DEVICE_POLICY_MANAGER]" };
        apps.find((a) => a.name === c.name)!.state = c.to;
        return { name: c.name, ok: true };
      });
      return { results, remaining: planFor(m).changes.length, undoSnapshot: metaOf(undo) };
    },
    exportJson(id: string): string { const m = snaps.find((x) => x.id === id)!; return JSON.stringify(m, null, 2); },
    importJson(text: string): SnapshotMeta {
      const m = JSON.parse(text) as MockSnap;
      if (!m.states) throw "That file is not a valid snapshot.";
      m.id = `${Date.now()}-imp`; m.name = `${m.name} (imported)`; m.auto = false; snaps.unshift(m);
      return metaOf(m);
    },
  },
  history: { load: [] as unknown[] },

  async deviceInfo(_serial: string): Promise<DeviceInfo> {
    await delay(150);
    return { manufacturer: "Google", model: "Pixel 7 (mock)", androidRelease: "14", sdk: 34, securityPatch: "2024-05-05",
      storageTotalKb: 113_311_596, storageUsedKb: 71_000_000, storageFreeKb: 42_000_000, batteryLevel: 81 };
  },
  _ht: 0,
  async healthSample(_serial: string): Promise<HealthSample> {
    await delay(120);
    this._ht++;
    const t = this._ht;
    const hot = 41 + Math.min(t, 40) * 0.25 + Math.sin(t / 4) * 2 + Math.random();
    const batt = 31 + Math.min(t, 60) * 0.1 + Math.sin(t / 9);
    const status = hot > 49 ? 2 : hot > 45 ? 1 : 0;
    const sensors = [
      { name: "cpu-1-0-usr", tempC: hot, source: "zone" },
      { name: "gpuss-0", tempC: hot - 3.2, source: "zone" },
      { name: "battery", tempC: batt, source: "hal" },
      { name: "skin", tempC: batt + 2.1, source: "hal" },
    ];
    return {
      battery: { level: 81, tempC: batt, status: "charging", health: "good", charging: true, powerSource: "usb", voltageMv: 4012 },
      thermalStatus: status,
      thermalStatusName: ["none", "light", "moderate"][status],
      sensors, hottest: sensors[0],
      cpu: Array.from({ length: 8 }, (_, i) => {
        const max = i < 4 ? 1_800_000 : i < 7 ? 2_400_000 : 2_850_000;
        const cur = Math.round(max * (status >= 2 ? 0.55 : 0.4 + 0.5 * Math.abs(Math.sin(t / 3 + i))));
        return { core: `cpu${i}`, curKhz: cur, maxKhz: max };
      }),
      missing: [],
    };
  },
};
