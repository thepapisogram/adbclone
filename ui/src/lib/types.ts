export type PkgState = "enabled" | "disabled" | "uninstalled";
export type Risk = "recommended" | "advanced" | "unsafe" | "unknown";
export type Action = "disable" | "enable" | "uninstall" | "restore" | "stop";
export type Tier = "safe" | "balanced" | "aggressive" | "never" | "unrated";
export type Confidence = "high" | "medium" | "low";
export interface Protection { package: string; reason: string }
export type Safety = "safe" | "caution" | "unsafe" | "unknown";

export interface Device {
  serial: string;
  state: "device" | "unauthorized" | "offline" | string;
  model?: string | null;
}

export interface AppInfo {
  name: string;
  label: string;
  system: boolean;
  state: PkgState;
  risk: Risk;
  disableSafety: Safety;
  uninstallSafety: Safety;
  description?: string | null;
  /** How much you would miss it: safe, balanced, aggressive, never (not offered) or unrated. */
  tier: Tier;
  category?: string | null;
  /** One line: what stops working if it is removed. */
  impact?: string | null;
  confidence: Confidence;
  /** `store` (reinstall from an app store) or `rom-only` (restore from a snapshot). */
  reinstall?: string | null;
  duplicateOf?: string | null;
  /** Where the shown name came from; unset means it is only a guess from the package id. */
  nameSource?: NameSource;
  /** Facts reported by the on-device helper. */
  details?: AppDetails;
}

export interface AppDetails {
  launchable: boolean; installer: string; versionName: string; versionCode: number;
  firstInstall: number; lastUpdate: number; targetSdk: number; updatedSystem: boolean;
}

export interface ActionResult {
  name: string;
  ok: boolean;
  message?: string;
}

export interface RunningRow {
  package: string | null; // null = system processes bucket
  rssKb: number;
  cpu: number | null; // percent of one core; null on the first sample
  processes: number;
  risk: Risk;
  disableSafety: Safety;
  uninstallSafety: Safety;
  description?: string | null;
}

export interface RunningResponse {
  rows: RunningRow[];
  memTotalKb: number;
  memAvailableKb: number;
  cores: number;
}

export type NameSource = "helper" | "file" | "cache";
export interface LabelItem { name: string; label: string; source?: NameSource }
export interface LabelProgress { done: number; total: number }

export interface MdnsService { name: string; kind: "pairing" | "connect" | "other"; address: string }
export interface SavedDevice { name: string; address: string; mdnsName?: string | null }
export interface AdbCheck { major: number | null; ok: boolean; message: string }
export interface QrStart { svg: string; name: string; password: string; id: number }

/** Wireless devices have an ip:port serial, or an mDNS name like `adb-XXXX._adb-tls-connect._tcp`. */
export const isWireless = (serial: string) => serial.includes(":") || serial.startsWith("adb-");

export type Step = "restore" | "enable" | "disable" | "uninstall";
export interface SnapshotMeta {
  id: string; name: string; created: number; auto: boolean; deviceModel?: string | null;
  enabled: number; disabled: number; uninstalled: number;
}
export interface Change { name: string; from: PkgState; to: PkgState; steps: Step[] }
export interface PlanResponse {
  plan: { changes: Change[]; missing: string[]; unchanged: number };
  snapshotName: string; snapshotModel?: string | null; deviceModel?: string | null;
}
export interface RestoreResponse { results: ActionResult[]; remaining: number; undoSnapshot?: SnapshotMeta | null }

export interface DeviceInfo {
  manufacturer?: string | null; model?: string | null; androidRelease?: string | null; sdk?: number | null;
  securityPatch?: string | null; storageTotalKb?: number | null; storageUsedKb?: number | null; storageFreeKb?: number | null;
  batteryLevel?: number | null;
}
export interface Battery {
  level: number | null; tempC: number | null; status: string; health: string; charging: boolean;
  powerSource: string; voltageMv: number | null;
}
export interface Sensor { name: string; tempC: number; source: string }
export interface CpuFreq { core: string; curKhz: number; maxKhz: number }
export interface HealthSample {
  battery: Battery | null; thermalStatus: number | null; thermalStatusName: string | null;
  sensors: Sensor[]; hottest: Sensor | null; cpu: CpuFreq[]; missing: string[];
}

export interface TuningStatus { animationScale?: string | null; transitionScale?: string | null; animatorScale?: string | null; batterySaver?: string | null; peakRefresh?: string | null; maxFps?: number | null }
export type TuningTarget = { kind: "setting"; ns: string; key: string } | { kind: "appOp"; pkg: string; op: string } | { kind: "standby"; pkg: string };
export interface TuningRecord { id: string; serial: string; group: string; label: string; target: TuningTarget; prev?: string | null; applied: string; time: number }
export interface TuningInfo { status: TuningStatus; refreshSupported: boolean; records: TuningRecord[] }
export interface StepResult { label: string; ok: boolean; message?: string | null }

export interface ReportSection { name: string; ok: boolean; bytes: number }
export interface ReportOut { text: string; sections: ReportSection[] }

export interface Unresolved { name: string; reason: string }
export interface NamesStatusInfo { helper: number; file: number; cached: number; unresolved: Unresolved[]; helperMessage?: string | null; finished: boolean }
export interface DetailItem extends AppDetails { name: string }
export interface IconItem { name: string; png: string; mime?: string }
export interface LabelHandlers {
  onLabels: (l: LabelItem[]) => void;
  onProgress: (p: LabelProgress) => void;
  onStatus: (s: NamesStatusInfo) => void;
  onDetails: (d: DetailItem[]) => void;
  onIcons: (i: IconItem[]) => void;
}

export interface CommandLogEntry { id: number; seq: number; timeMs: number; command: string; ok: boolean; millis: number; count: number; note?: string | null }
export interface CommandLog { entries: CommandLogEntry[]; seq: number }

export interface AdbStatus { found: boolean; source?: "env" | "appData" | "nextToApp" | "path" | null; path?: string | null; version?: string | null; major?: number | null }
export interface AdbDownloadEvent { stage: "downloading" | "unpacking" | "done" | "error"; received: number; total?: number | null; message?: string | null }

export interface InstallItem { id: string; source: string; kind: string; fileCount: number; package?: string | null; version?: string | null; label?: string | null; permissions: number; sizeBytes: number; warnings: string[] }
export interface InspectOut { item?: InstallItem | null; error?: string | null }
export interface InstallOptions { replace: boolean; allowDowngrade: boolean; allowTest: boolean }
export interface ExportResult { package: string; folder: string; files: string[]; bytes: number }

export interface AppStorage { package: string; appBytes: number; dataBytes: number; cacheBytes: number }
export interface StorageReport { available: boolean; apps: AppStorage[] }
export interface Usage { package: string; lastUsed: string; daysAgo: number; launches?: number | null }
export interface UsageReport { available: boolean; entries: Usage[] }

export interface WakeLockRow { package?: string | null; uid: number; kind: string; tag: string; heldSecs?: number | null }
export interface Wakeups { package: string; wakeups: number; alarms: number }
export interface WakeReport { available: boolean; locks: WakeLockRow[]; wakeups: Wakeups[] }
