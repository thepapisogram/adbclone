/** A short, local history of readings so a change can be judged by numbers, not feelings. */
export interface Entry {
  t: number; label: string;
  hot: number | null; batt: number | null; level: number | null; charging: boolean | null; availKb: number | null;
}
const MAX = 40;
const key = (serial: string) => `coolant.history.${serial}`;

export function loadHistory(serial: string): Entry[] {
  try {
    const v = JSON.parse(localStorage.getItem(key(serial)) ?? "[]");
    return Array.isArray(v) ? v.filter((e) => e && typeof e.t === "number" && typeof e.label === "string").slice(-MAX) : [];
  } catch { return []; }
}
export function addHistory(serial: string, e: Entry): Entry[] {
  const all = [...loadHistory(serial), e].slice(-MAX);
  try { localStorage.setItem(key(serial), JSON.stringify(all)); } catch { /* storage unavailable: history stays in memory only */ }
  return all;
}
export function clearHistory(serial: string): Entry[] {
  try { localStorage.removeItem(key(serial)); } catch { /* ignore */ }
  return [];
}

/** Battery points lost per hour between two readings; null when it cannot be told fairly
 *  (charging at either time, battery went up, or the readings are less than 5 minutes apart). */
export function drainPerHour(a: Entry, b: Entry): number | null {
  if (a.level == null || b.level == null || a.charging !== false || b.charging !== false) return null;
  const hours = (b.t - a.t) / 3_600_000;
  if (hours < 5 / 60 || b.level > a.level) return null;
  return (a.level - b.level) / hours;
}

export const isWarm = (hot: number | null, batt: number | null, thermalStatus: number | null) =>
  (thermalStatus ?? 0) >= 2 || (hot ?? 0) >= 42 || (batt ?? 0) >= 40;
