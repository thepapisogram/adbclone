export type PkgState = "enabled" | "disabled" | "uninstalled";
export type Risk = "recommended" | "advanced" | "unsafe" | "unknown";
export type Action = "disable" | "enable" | "uninstall" | "restore";

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
  description?: string;
}

export interface ActionResult {
  name: string;
  ok: boolean;
  message?: string;
}
