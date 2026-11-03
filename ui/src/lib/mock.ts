import type { RatingsOverview, ImportOut, UserRating, CommunityFile, WakeReport, StorageReport, UsageReport, ExportResult, InspectOut, InstallItem, InstallOptions, Protection, Action, ActionResult, AdbCheck, AdbDownloadEvent, AdbStatus, CommandLog, CommandLogEntry, LabelHandlers, ReportOut, StepResult, TuningInfo, TuningRecord, DeviceInfo, HealthSample, Change, PlanResponse, RestoreResponse, SnapshotMeta, AppInfo, Device, MdnsService, QrStart, RunningResponse, Safety, SavedDevice } from "./types";

const IMPACT: Record<string, string> = {
  "ads-telemetry": "Stops usage reports, ads or diagnostics. Nothing you use directly.",
  "partner-stub": "A helper for a preinstalled partner app. Nothing else depends on it; the app itself keeps working or can be reinstalled.",
  "help-promo": "A tips, help or promotional app. Nothing else depends on it.",
  "game-hub": "You lose the game launcher or its overlay. Games still run.",
  "aosp-extra": "A leftover extra such as a screensaver, live wallpaper or test tool. Nothing you use depends on it.",
  assistant: "The voice assistant and its wake word stop working.",
  "google-app": "You lose this Google app. You can reinstall it from the Play Store.",
  "preinstalled-app": "You lose this app. You can reinstall it from the store.",
  store: "Installs and updates from this store stop. Apps you already have keep working.",
  "system-service": "Features of the maker's system (battery care, security scan, cleaner, sync, setup) may stop working.",
  "core-app": "One of the phone's main apps (calls, messages, camera, keyboard, browser, files, clock or home screen). Make sure another one is set up first.",
  "core-system": "Can stop the phone from working properly. Never remove.",
};
type Row = [name: string, label: string, system: boolean, risk: AppInfo["risk"], tier: AppInfo["tier"], category: string | null, confidence: AppInfo["confidence"], description: string, duplicateOf?: string];
const rows: Row[] = [
  ["com.google.android.youtube", "YouTube", true, "recommended", "balanced", "google-app", "high", "Video app; reinstallable from Play Store."],
  ["com.google.android.apps.maps", "Maps", true, "recommended", "balanced", "google-app", "medium", "Google Maps."],
  ["com.google.android.gm", "Gmail", true, "advanced", "balanced", "google-app", "high", "Email client."],
  ["com.google.android.gms", "Google Play services", true, "unsafe", "never", "core-system", "high", "Core services; many apps break without it."],
  ["com.google.android.gsf", "Google Services Framework", true, "unsafe", "never", "core-system", "high", "Required for Google account sync and push."],
  ["com.android.vending", "Google Play Store", true, "advanced", "aggressive", "store", "high", "App store."],
  ["com.android.systemui", "System UI", true, "unsafe", "never", "core-system", "high", "Status bar, navigation and lock screen. Disabling bricks the UI."],
  ["com.android.settings", "Settings", true, "unsafe", "never", "core-system", "high", "Device settings."],
  ["com.android.phone", "Phone Services", true, "unsafe", "never", "core-system", "high", "Telephony stack."],
  ["com.android.chrome", "Chrome", true, "advanced", "aggressive", "core-app", "high", "Default browser."],
  ["com.facebook.appmanager", "Facebook App Manager", true, "recommended", "safe", "partner-stub", "high", "Preinstalled background updater."],
  ["com.facebook.services", "Facebook Services", true, "recommended", "safe", "partner-stub", "high", "Preinstalled background service."],
  ["com.facebook.system", "Facebook Installer", true, "recommended", "safe", "partner-stub", "high", "Preinstalled installer stub."],
  ["com.samsung.android.bixby.agent", "Bixby Voice", true, "recommended", "balanced", "assistant", "high", "Voice assistant."],
  ["com.samsung.android.game.gamehome", "Game Launcher", true, "recommended", "balanced", "game-hub", "medium", "Game launcher overlay."],
  ["com.microsoft.office.officehubrow", "Microsoft 365", true, "recommended", "balanced", "preinstalled-app", "medium", "Preinstalled Office hub."],
  ["com.netflix.partner.activation", "Netflix Activation", true, "recommended", "safe", "partner-stub", "high", "Partner activation stub."],
  ["com.google.android.apps.photos", "Photos", true, "advanced", "balanced", "google-app", "medium", "Gallery and backup."],
  ["com.android.stk", "SIM Toolkit", true, "advanced", "aggressive", "system-service", "medium", "Carrier SIM menu."],
  ["com.android.egg", "Android Easter Egg", true, "recommended", "safe", "aosp-extra", "high", "A hidden game."],
  ["com.samsung.android.app.tips", "Tips", true, "recommended", "safe", "help-promo", "high", "Tips and tutorials."],
  ["com.vendor.analyticsservice", "Vendor Analytics", true, "recommended", "safe", "ads-telemetry", "medium", "Usage reports sent to the maker."],
  ["com.sec.android.gallery3d", "Samsung Gallery", true, "advanced", "balanced", "vendor-app", "medium", "Photo gallery.", "com.google.android.apps.photos"],
  ["com.sec.android.app.sbrowser", "Samsung Internet", true, "advanced", "aggressive", "core-app", "medium", "Samsung browser.", "com.android.chrome"],
  ["com.vendor.unknownservice", "Vendor Service", true, "unknown", "unrated", null, "low", ""],
  ["com.sec.android.app.randomthing", "Random Thing", true, "unknown", "unrated", null, "low", ""],
  ["org.mozilla.firefox", "Firefox", false, "unknown", "unrated", null, "low", ""],
  ["com.spotify.music", "Spotify", false, "unknown", "unrated", null, "low", ""],
  ["org.telegram.messenger", "Telegram", false, "unknown", "unrated", null, "low", ""],
  ["com.whatsapp", "WhatsApp", false, "unknown", "unrated", null, "low", ""],
  ["com.discord", "Discord", false, "unknown", "unrated", null, "low", ""],
  ["com.instagram.android", "Instagram", false, "unknown", "unrated", null, "low", ""],
];
const safetyOf = (risk: string): Safety => (({ recommended: "safe", advanced: "caution", unsafe: "unsafe", unknown: "unknown" }) as Record<string, Safety>)[risk];
const apps: AppInfo[] = rows.map(([name, label, system, risk, tier, category, confidence, description, duplicateOf]) => ({
  name, label, system, risk, tier, category, confidence, description, duplicateOf: duplicateOf ?? null,
  impact: category ? IMPACT[category] ?? null : null,
  disableSafety: safetyOf(risk),
  uninstallSafety: safetyOf(risk),
  state: "enabled" as const,
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

/** For tests: a second ready phone appears when this browser flag is set. */
const second = () => { try { return localStorage.getItem("coolant.mock.second") === "1"; } catch { return false; } };

export const mock = {
  async listDevices(): Promise<Device[]> {
    await delay(100);
    return [
      { serial: "R58M123ABC", state: "device", model: "Pixel 7 (mock)" },
      { serial: "192.168.1.30:5555", state: "unauthorized", model: null },
      ...[...wifi].map((serial) => ({ serial, state: "device", model: "Pixel 7 (mock, Wi-Fi)" })),
      ...(second() ? [{ serial: "R99SECOND", state: "device", model: "Galaxy S23 (mock)" }] : []),
    ];
  },
  async listApps(_serial: string): Promise<AppInfo[]> {
    await delay();
    // Like the real backend, start with names guessed from the package id.
    return apps.map((a) => ({ ...a, label: guess(a.name) }));
  },
  /** Streams names, details and icons in a few batches, like the Rust backend does. */
  loadLabels(h: LabelHandlers) {
    const total = apps.length;
    const batches = 5;
    const size = Math.ceil(total / batches);
    const colors = ["#2563eb", "#16a34a", "#d97706", "#9333ea", "#dc2626", "#0891b2"];
    const icon = (name: string, label: string) => {
      const c = colors[[...name].reduce((x, ch) => x + ch.charCodeAt(0), 0) % colors.length];
      const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><rect width="64" height="64" rx="14" fill="${c}"/><text x="32" y="43" font-size="30" text-anchor="middle" fill="#fff" font-family="sans-serif">${label.charAt(0).toUpperCase()}</text></svg>`;
      return { name, png: btoa(svg), mime: "image/svg+xml" };
    };
    // Apps the helper "cannot name" (to show the unresolved list); everything else gets a real name.
    const unnamed = new Set(["com.vendor.unknownservice"]);
    for (let i = 0; i < batches; i++) {
      setTimeout(() => {
        const part = apps.slice(i * size, (i + 1) * size);
        h.onLabels(part.filter((a) => !unnamed.has(a.name)).map((a) => ({ name: a.name, label: a.label, source: i === 4 ? "file" as const : "helper" as const })));
        h.onDetails(part.map((a, k) => ({ name: a.name, launchable: !a.system || k % 3 === 0, installer: a.system ? "" : "com.android.vending", versionName: "1." + k, versionCode: 100 + k, firstInstall: 1_700_000_000_000, lastUpdate: 1_710_000_000_000, targetSdk: 34, updatedSystem: false })));
        h.onProgress({ done: Math.min(total, (i + 1) * size), total });
      }, 500 + i * 450);
    }
    setTimeout(() => h.onStatus({ helper: total - 6 - 1, file: 6, cached: 0, unresolved: [{ name: "com.vendor.unknownservice", reason: "the app declares no name" }], helperMessage: null, finished: true }), 500 + batches * 450);
    apps.forEach((a, i) => setTimeout(() => h.onIcons([icon(a.name, a.label)]), 700 + i * 60));
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

  tuning: {
    records: [] as TuningRecord[],
    add(group: string, label: string, target: TuningRecord["target"], applied: string) {
      this.records = this.records.filter((r) => JSON.stringify(r.target) !== JSON.stringify(target));
      this.records.push({ id: Math.random().toString(36).slice(2, 10), serial: "R58M123ABC", group, label, target, prev: null, applied, time: Math.floor(Date.now() / 1000) });
    },
    async status(): Promise<TuningInfo> {
      await delay(150);
      const has = (g: string) => this.records.some((r) => r.group === g);
      const sc = (i: number, d: string) => (has("animations") ? this.anim[i] : d);
      return { status: { animationScale: sc(0, "1.0"), transitionScale: sc(1, "1.0"), animatorScale: sc(2, "1.0"), batterySaver: has("battery_saver") ? "1" : "0", peakRefresh: has("refresh") ? "60.0" : null, maxFps: 120 }, refreshSupported: true, records: this.records.map((r) => ({ ...r })) };
    },
    anim: ["0.5", "0.5", "0.5"],
    async animations(_s: string, w: string, t: string, a: string): Promise<StepResult[]> {
      await delay(300);
      this.anim = [w, t, a];
      this.records = this.records.filter((r) => r.group !== "animations");
      this.add("animations", "Window animation speed", { kind: "setting", ns: "global", key: "window_animation_scale" }, w);
      return [{ label: "Window animation speed", ok: true }, { label: "Transition animation speed", ok: true }, { label: "In-app animation speed", ok: true }];
    },
    async toggle(_s: string, id: string, on: boolean): Promise<StepResult[]> {
      await delay(400);
      if (on && id === "animations") this.anim = ["0.5", "0.5", "0.5"];
      const labels: Record<string, string> = { animations: "Animations 0.5x", battery_saver: "Battery saver on", refresh: "Refresh rate capped at 60 Hz" };
      if (on) { this.add(id, labels[id], { kind: "setting", ns: "global", key: id }, "on"); return [{ label: labels[id], ok: true }]; }
      this.records = this.records.filter((r) => r.group !== id);
      return [{ label: `Undone: ${labels[id]}`, ok: true }];
    },
    async restrict(_s: string, names: string[], restrict: boolean): Promise<StepResult[]> {
      await delay(500);
      return names.map((n) => {
        if (restrict) {
          if (n.startsWith("com.google.android.gms") || n === "com.android.systemui") return { label: `Restrict background: ${n}`, ok: false, message: "Rated unsafe: restricting it could break calls, notifications or sync." };
          this.add("background", "Background activity blocked", { kind: "appOp", pkg: n, op: "RUN_ANY_IN_BACKGROUND" }, "deny");
          return { label: `Restrict background: ${n}`, ok: true };
        }
        this.records = this.records.filter((r) => !(r.group === "background" && "pkg" in r.target && r.target.pkg === n));
        return { label: `Undone: ${n}`, ok: true };
      });
    },
    async undo(_s: string, f: { group?: string | null; id?: string | null; package?: string | null }): Promise<StepResult[]> {
      await delay(400);
      const hit = (r: TuningRecord) => (!f.group || r.group === f.group) && (!f.id || r.id === f.id) && (!f.package || ("pkg" in r.target && r.target.pkg === f.package));
      const gone = this.records.filter(hit);
      this.records = this.records.filter((r) => !hit(r));
      return gone.map((r) => ({ label: `Undone: ${r.label}`, ok: true }));
    },
    async cooldown(_s: string, apps: string[], stop: boolean): Promise<StepResult[]> {
      await delay(900);
      this.add("battery_saver", "Battery saver on", { kind: "setting", ns: "global", key: "low_power" }, "1");
      this.add("animations", "Animations 0.5x", { kind: "setting", ns: "global", key: "window_animation_scale" }, "0.5");
      this.add("refresh", "Refresh rate capped at 60 Hz", { kind: "setting", ns: "system", key: "peak_refresh_rate" }, "60.0");
      const out: StepResult[] = [{ label: "Battery saver on", ok: true }, { label: "Animations 0.5x", ok: true }, { label: "Refresh rate capped at 60 Hz", ok: true }];
      out.push(...(await this.restrict("", apps, true)));
      if (stop) out.push(...apps.map((n) => ({ label: `Stop ${n}`, ok: true })));
      return out;
    },
  },

  async report(): Promise<ReportOut> {
    await delay(1200);
    const names = ["build info", "device info (parser input)", "health (parser input)", "processes (parser input)", "tuning status (parser input)", "system packages", "package counts", "tool availability", "app ops and standby sample", "wifi address", "adb on this computer"];
    const text = ["adbclone device report", "app version: 0.1.0 (mock)", "created (unix time): 1700000000", "This report only reads information from the phone; nothing was changed.",
      "Serial numbers, e-mail addresses, IP/MAC addresses and long ID numbers were replaced with placeholders.", "Read it before sharing.", "",
      ...names.flatMap((n) => [`=== ${n} [ok] ===`, "$ (mock command)", "(sample output)", ""])].join("\n");
    return { text, sections: names.map((name) => ({ name, ok: true, bytes: 120 })) };
  },

  log: {
    entries: [] as CommandLogEntry[],
    seq: 0,
    id: 0,
    add(command: string, ok = true, note: string | null = null) {
      const last = this.entries[this.entries.length - 1];
      this.seq++;
      if (last && last.command === command) { last.count++; last.seq = this.seq; last.timeMs = Date.now(); return; }
      this.entries.push({ id: ++this.id, seq: this.seq, timeMs: Date.now(), command, ok, millis: 20 + Math.round(Math.random() * 80), count: 1, note });
    },
    async since(seq: number): Promise<CommandLog> {
      // Looks like the real thing: the app's background polling shows up here.
      this.add("adb devices -l");
      if (this.seq % 4 === 0) this.add("adb -s R58M123ABC shell pm list packages -f -U --user 0");
      if (this.seq % 7 === 0) this.add("adb -s R58M123ABC shell settings put global low_power 1", false, "device offline");
      return { entries: this.entries.filter((e) => e.seq > seq).map((e) => ({ ...e })), seq: this.seq };
    },
    async clear() { this.entries = []; this.seq++; },
  },

  adb: {
    // Open the page with ?noadb=1 to see what a computer without adb looks like.
    installed: !new URLSearchParams(typeof location === "undefined" ? "" : location.search).has("noadb"),
    failNext: new URLSearchParams(typeof location === "undefined" ? "" : location.search).has("adbfail"),
    async status(): Promise<AdbStatus> {
      await delay(100);
      return this.installed ? { found: true, source: "appData", path: "C:/Users/you/AppData/Roaming/Coolant/platform-tools/adb.exe", version: "Version 35.0.2-12147458", major: 35 } : { found: false };
    },
    download(on: (e: AdbDownloadEvent) => void) {
      const total = 8_000_000;
      let got = 0;
      const t = setInterval(() => {
        got += 1_000_000;
        if (this.failNext && got >= 3_000_000) { clearInterval(t); this.failNext = false; on({ stage: "error", received: got, total, message: "could not connect: the network is unreachable" }); return; }
        if (got < total) on({ stage: "downloading", received: got, total });
        else { clearInterval(t); this.installed = true; on({ stage: "done", received: total, total }); }
      }, 250);
    },
  },

  async protections(_serial: string): Promise<Protection[]> {
    await delay(100);
    return [{ package: "com.android.chrome", reason: "your browser" }];
  },

  install: {
    queue: new Map<string, InstallItem>(),
    pick(kind: "files" | "folder"): string[] {
      return kind === "folder" ? ["C:/Backups/com.example.tool"] : ["C:/Downloads/Hello Notes.apk", "C:/Downloads/Big Game.xapk", "C:/Downloads/already-exists.apk", "C:/Downloads/readme.txt"];
    },
    async inspect(paths: string[]): Promise<InspectOut[]> {
      await delay(300);
      return paths.map((p) => {
        const name = p.split(/[\\/]/).pop() ?? p;
        const ext = name.split(".").pop()?.toLowerCase();
        if (!["apk", "apks", "xapk", "apkm"].includes(ext ?? "") && !p.endsWith("tool")) return { error: `${name}: only .apk, .apks, .xapk and .apkm files, or folders of .apk files, can be installed` };
        const base = name.replace(/\.[^.]+$/, "");
        const item: InstallItem = {
          id: Math.random().toString(36).slice(2, 10), source: name, kind: p.endsWith("tool") ? "folder" : ext === "xapk" ? "xapk" : "apk", fileCount: ext === "xapk" ? 3 : 1,
          package: `com.example.${base.toLowerCase().replace(/[^a-z0-9]/g, "")}`, version: "2.4.1", label: base, permissions: 7, sizeBytes: ext === "xapk" ? 412_000_000 : 18_500_000,
          warnings: ext === "xapk" ? ["This file also contains game data (OBB files) that is not copied. The app may need it to run."] : [],
        };
        this.queue.set(item.id, item);
        return { item };
      });
    },
    async run(_serial: string, id: string, o: InstallOptions): Promise<string> {
      await delay(900);
      const it = this.queue.get(id);
      if (it?.source.includes("exists") && !o.replace) throw "This app is already installed. Tick \"Replace if already installed\". (INSTALL_FAILED_ALREADY_EXISTS)";
      return `${it?.label ?? "App"} installed.`;
    },
    async discard(ids: string[]) { ids.forEach((i) => this.queue.delete(i)); },
  },
  backup: {
    async one(_serial: string, pkg: string, dest: string): Promise<ExportResult> {
      await delay(350);
      if (pkg === "com.whatsapp") throw "This app is not installed for you right now, so its files cannot be copied.";
      const split = pkg.length % 2 === 0;
      return { package: pkg, folder: `${dest}/${pkg}`, files: split ? ["base.apk", "split_config.arm64_v8a.apk", "split_config.xxhdpi.apk"] : ["base.apk"], bytes: split ? 61_000_000 : 24_000_000 };
    },
  },

  async storage(_serial: string): Promise<StorageReport> {
    await delay(300);
    const apps = this_apps().map((a, i) => ({ package: a.name, appBytes: (40 + ((i * 53) % 400)) * 1_048_576, dataBytes: ((i * 37) % 300) * 1_048_576, cacheBytes: ((i * 71) % 180) * 1_048_576 }));
    return { available: true, apps: apps.sort((a, b) => b.appBytes + b.dataBytes + b.cacheBytes - (a.appBytes + a.dataBytes + a.cacheBytes)) };
  },
  async screenshot(_serial: string): Promise<string> {
    await delay(300);
    const c = Object.assign(document.createElement("canvas"), { width: 270, height: 480 });
    const g = c.getContext("2d");
    if (g) { g.fillStyle = "#1e3a8a"; g.fillRect(0, 0, 270, 480); g.fillStyle = "#fff"; g.fillText("mock screenshot", 80, 240); }
    const a = Object.assign(document.createElement("a"), { href: c.toDataURL("image/png"), download: "screenshot.png" });
    a.click();
    return "screenshot.png";
  },
  async mirrorAvailable(): Promise<boolean> { return true; },
  async mirror(serial: string): Promise<void> { await delay(300); if (serial === "NOSCRCPY") throw "scrcpy was not found."; },
  ratings: {
    community: [] as CommunityFile[],
    user: [] as UserRating[],
    async overview(): Promise<RatingsOverview> { return { community: [...this.community], user: [...this.user] }; },
    async importJson(name: string, text: string): Promise<ImportOut> {
      let v: unknown;
      try { v = JSON.parse(text); } catch { throw "That is not a Coolant ratings file (expected a list of ratings)."; }
      if (!Array.isArray(v)) throw "That is not a Coolant ratings file (expected a list of ratings).";
      const ok = v.filter((e) => e && typeof e.id === "string" && /^[A-Za-z0-9._]+$/.test(e.id) && !e.prefix && ["recommended", "advanced", "unsafe"].includes(e.risk) && !["com.android.systemui", "android"].includes(e.id));
      if (!ok.length) throw "No usable ratings were found in that file.";
      const file = name.replace(/[^\w-]+/g, "_").replace(/_json$/, "") + ".json";
      this.community = this.community.filter((c) => c.file !== file).concat({ file, entries: ok.length });
      return { file, accepted: ok.length, skipped: v.length - ok.length };
    },
    async removeCommunity(file: string) { this.community = this.community.filter((c) => c.file !== file); },
    async setUser(id: string, risk: string, desc: string) {
      if (["android", "com.android.systemui", "com.android.phone"].includes(id)) throw "This package cannot be rated: it is a core system package, or the name is not valid.";
      this.user = this.user.filter((u) => u.id !== id).concat({ id, risk: risk as UserRating["risk"], desc });
    },
    async removeUser(id: string) { this.user = this.user.filter((u) => u.id !== id); },
  },
  async wake(_serial: string): Promise<WakeReport> {
    await delay(300);
    return {
      available: true,
      locks: [
        { package: "com.whatsapp", uid: 10101, kind: "PARTIAL_WAKE_LOCK", tag: "WhatsApp:sync", heldSecs: 340 },
        { package: "com.facebook.katana", uid: 10102, kind: "PARTIAL_WAKE_LOCK", tag: "FacebookService", heldSecs: 95 },
        { package: null, uid: 1001, kind: "PARTIAL_WAKE_LOCK", tag: "RILJ_ACK_WL", heldSecs: 4 },
      ],
      wakeups: [{ package: "com.facebook.katana", wakeups: 412, alarms: 900 }, { package: "com.whatsapp", wakeups: 120, alarms: 300 }, { package: "com.google.android.gms", wakeups: 90, alarms: 400 }],
    };
  },
  async usage(_serial: string): Promise<UsageReport> {
    await delay(300);
    // Some apps have no record at all (the phone only keeps a limited history).
    const days: Record<string, number> = { "com.android.chrome": 0, "com.whatsapp": 0, "com.spotify.music": 2, "org.mozilla.firefox": 45, "com.discord": 95, "com.instagram.android": 12, "com.google.android.youtube": 70, "com.google.android.apps.maps": 33, "com.samsung.android.game.gamehome": 200 };
    return { available: true, entries: Object.entries(days).map(([package_, daysAgo]) => ({ package: package_, daysAgo, lastUsed: "2024-06-30 12:00:00", launches: 3 })) };
  },
};
function this_apps() { return apps; }
