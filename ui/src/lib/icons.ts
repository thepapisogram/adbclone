import { useSyncExternalStore } from "react";
import type { IconItem } from "./types";

/** App icons by package name, as data URLs. A tiny store so any table can show an icon as it arrives. */
const icons = new Map<string, string>();
const listeners = new Set<() => void>();
let version = 0;

export function addIcons(items: IconItem[]) {
  for (const i of items) icons.set(i.name, `data:${i.mime ?? "image/png"};base64,${i.png}`);
  version++;
  listeners.forEach((l) => l());
}
export function clearIcons() { icons.clear(); version++; listeners.forEach((l) => l()); }

function subscribe(cb: () => void) { listeners.add(cb); return () => { listeners.delete(cb); }; }
/** Re-renders the caller whenever icons change; returns the icon for `name` if we have one. */
export function useIcon(name: string): string | undefined {
  useSyncExternalStore(subscribe, () => version);
  return icons.get(name);
}
