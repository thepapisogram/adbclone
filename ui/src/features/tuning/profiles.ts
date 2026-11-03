import { PRESETS, type Mix } from "./animations";

export interface Profile { id: string; title: string; text: string; animations: Mix | "normal"; batterySaver: boolean; refreshCap: boolean }
const mixOf = (id: string) => PRESETS.find((p) => p.id === id)!.mix;

export const PROFILES: Profile[] = [
  { id: "gaming", title: "Gaming", text: "Takes off the limits this app set: battery saver off, screen at its full refresh rate, normal animations. It cannot make the phone faster than it is, and it may run warmer.", animations: "normal", batterySaver: false, refreshCap: false },
  { id: "battery", title: "Battery saver", text: "Battery saver on, screen capped at 60 Hz, quicker animations. Uses less power and makes less heat.", animations: mixOf("snappy"), batterySaver: true, refreshCap: true },
  { id: "work", title: "Work", text: "Quick and smooth animations and a 60 Hz screen, with battery saver off so nothing is delayed.", animations: mixOf("quick"), batterySaver: false, refreshCap: true },
];

export type Op = { kind: "animations"; mix: Mix } | { kind: "toggle"; id: "animations" | "battery_saver" | "refresh"; on: boolean };

/** The exact steps a profile runs. The 60 Hz cap is skipped on screens that have nothing to cap. */
export function profileOps(p: Profile, refreshSupported: boolean): Op[] {
  const ops: Op[] = [p.animations === "normal" ? { kind: "toggle", id: "animations", on: false } : { kind: "animations", mix: p.animations }];
  ops.push({ kind: "toggle", id: "battery_saver", on: p.batterySaver });
  if (refreshSupported) ops.push({ kind: "toggle", id: "refresh", on: p.refreshCap });
  return ops;
}
