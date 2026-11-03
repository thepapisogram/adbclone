import type { Device } from "./types";

export interface PickResult { serial: string; misses: number }

/** How many polls in a row a selected phone may be missing before we give up on it. */
export const MAX_MISSES = 3;

/**
 * Decide which device stays selected after a device-list refresh. A phone can show as "offline" or
 * vanish for a single poll (common over Wi-Fi); dropping the selection then would throw away the app
 * list and the loaded names, so the current phone is kept until it has been gone for several polls.
 */
export function pickSerial(cur: string, devices: Device[], select: string | undefined, misses: number): PickResult {
  const ready = (s: string) => devices.some((d) => d.serial === s && d.state === "device");
  if (select && ready(select)) return { serial: select, misses: 0 };
  if (cur && ready(cur)) return { serial: cur, misses: 0 };
  if (cur && misses + 1 < MAX_MISSES) return { serial: cur, misses: misses + 1 };
  return { serial: devices.find((d) => d.state === "device")?.serial ?? "", misses: 0 };
}
