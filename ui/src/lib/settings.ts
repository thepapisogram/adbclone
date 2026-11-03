import { useSyncExternalStore } from "react";

export type ThemeChoice = "system" | "light" | "dark";
export interface Settings {
  theme: ThemeChoice;
  /** Plain-language mode: show app names and a one-line reason, hide package names and other technical text. */
  plain: boolean;
  language: string;
}

export const DEFAULT_SETTINGS: Settings = { theme: "system", plain: true, language: "en" };
const KEY = "coolant.settings";

/** Read settings from stored text, replacing anything missing or invalid with the default. Never throws. */
export function parseSettings(raw: string | null | undefined): Settings {
  let v: Record<string, unknown> = {};
  try { const p = raw ? JSON.parse(raw) : {}; if (p && typeof p === "object") v = p as Record<string, unknown>; } catch { /* use defaults */ }
  return {
    theme: v.theme === "light" || v.theme === "dark" || v.theme === "system" ? v.theme : DEFAULT_SETTINGS.theme,
    plain: typeof v.plain === "boolean" ? v.plain : DEFAULT_SETTINGS.plain,
    language: typeof v.language === "string" && /^[a-z]{2}(-[A-Za-z]{2,4})?$/.test(v.language) ? v.language : DEFAULT_SETTINGS.language,
  };
}

function load(): Settings {
  // Storage can be blocked or missing (private windows, previews, tests): the app must work without it.
  try { return parseSettings(globalThis.localStorage?.getItem(KEY)); } catch { return { ...DEFAULT_SETTINGS }; }
}

let current: Settings = load();
const listeners = new Set<() => void>();

export function getSettings(): Settings { return current; }

/** `system` removes the override so the OS preference applies. */
export function applyTheme(theme: ThemeChoice = current.theme) {
  if (typeof document === "undefined") return;
  if (theme === "system") delete document.documentElement.dataset.theme;
  else document.documentElement.dataset.theme = theme;
}

export function updateSettings(patch: Partial<Settings>) {
  current = parseSettings(JSON.stringify({ ...current, ...patch }));
  try { globalThis.localStorage?.setItem(KEY, JSON.stringify(current)); } catch { /* not persisted; still applied */ }
  applyTheme();
  listeners.forEach((l) => l());
}

function subscribe(cb: () => void) { listeners.add(cb); return () => { listeners.delete(cb); }; }
export function useSettings(): Settings { return useSyncExternalStore(subscribe, getSettings); }
