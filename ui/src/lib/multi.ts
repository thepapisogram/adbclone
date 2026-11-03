import type { Action, AppInfo, Device } from "./types";

/** Other phones that are connected and ready (not offline or waiting for authorisation). */
export const otherReady = (devices: Device[], serial: string) => devices.filter((d) => d.serial !== serial && d.state === "device");

/** Whether an action makes sense for an app in a given state (same rule as the single-phone buttons). */
export const fits = (action: Action, state: AppInfo["state"]) =>
  action === "disable" ? state === "enabled" : action === "enable" ? state === "disabled" : action === "uninstall" ? state !== "uninstalled" : action === "restore" ? state === "uninstalled" : true;

/** Of the chosen packages, the ones another phone has in a state the action applies to. */
export function appliesTo(action: Action, names: string[], theirs: Pick<AppInfo, "name" | "state">[]): { names: string[]; skipped: number } {
  const state = new Map(theirs.map((a) => [a.name, a.state]));
  const ok = names.filter((n) => state.has(n) && fits(action, state.get(n)!));
  return { names: ok, skipped: names.length - ok.length };
}

/** Actions it is reasonable to repeat on other phones (stopping an app is not worth it). */
export const repeatable = (action: Action) => action === "disable" || action === "uninstall" || action === "enable" || action === "restore";
