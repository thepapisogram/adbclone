export type PkgState = "enabled" | "disabled" | "uninstalled";
export type Risk = "recommended" | "advanced" | "unsafe" | "unknown";
export type Action = "disable" | "enable" | "uninstall" | "restore" | "stop";
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

export interface LabelItem { name: string; label: string }
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
