import type { AppStorage } from "../../lib/types";

/**
 * Android reports app sizes from its own periodically refreshed cache, so right after "clear caches" the
 * phone still prints the old figures. We remember each app's cache size from just before the clean-up and,
 * until the phone reports a different number, show it as cleared. Once the phone re-measures, its number wins.
 */
const before = new Map<string, Map<string, number>>();

export const fmtFreed = (bytes: number) =>
  bytes >= 1_073_741_824 ? `${(bytes / 1_073_741_824).toFixed(2)} GB` : bytes >= 1_048_576 ? `${Math.round(bytes / 1_048_576)} MB` : `${Math.max(1, Math.round(bytes / 1024))} KB`;

/** Call with the figures read just before clearing. Returns how many bytes of cache that was. */
export function rememberCleared(serial: string, sizes: AppStorage[]): number {
  const m = new Map<string, number>();
  let total = 0;
  for (const s of sizes) if (s.cacheBytes > 0) { m.set(s.package, s.cacheBytes); total += s.cacheBytes; }
  const prev = before.get(serial);
  if (prev) for (const [k, v] of prev) if (!m.has(k)) m.set(k, v);
  before.set(serial, m);
  return total;
}

export function applyCleared(serial: string, sizes: AppStorage[]): AppStorage[] {
  const m = before.get(serial);
  if (!m) return sizes;
  return sizes.map((s) => (m.get(s.package) === s.cacheBytes && s.cacheBytes > 0 ? { ...s, cacheBytes: 0 } : s));
}

/** How many apps are still shown as cleared because the phone has not re-measured them. */
export function clearedCount(serial: string, raw: AppStorage[]): number {
  const m = before.get(serial);
  return m ? raw.filter((s) => m.get(s.package) === s.cacheBytes && s.cacheBytes > 0).length : 0;
}

export const forgetCleared = (serial: string) => { before.delete(serial); };
