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
