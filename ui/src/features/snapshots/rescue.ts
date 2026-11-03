import type { SnapshotMeta } from "../../lib/types";

/** The snapshot to offer first in an emergency: the newest one, whatever made it. */
export function latestSnapshot(list: SnapshotMeta[]): SnapshotMeta | null {
  return list.reduce<SnapshotMeta | null>((best, m) => (!best || m.created > best.created ? m : best), null);
}

export interface SafeMode { maker: string; steps: string[] }

/** How to start Android in Safe Mode, which turns off apps you installed. Generic first, then maker notes. */
export const SAFE_MODE: SafeMode[] = [
  { maker: "Most phones (Pixel, Motorola, Nokia, OnePlus…)", steps: ["Press and hold the power button until the power menu shows.", "Touch and hold \"Power off\" until \"Reboot to safe mode\" appears, then tap OK."] },
  { maker: "Samsung", steps: ["Power the phone off.", "Turn it on, and as soon as the Samsung logo shows, hold Volume Down until the lock screen appears. \"Safe mode\" shows at the bottom."] },
  { maker: "Xiaomi, Redmi, POCO", steps: ["Power the phone off.", "Turn it on and hold Volume Down and the power button together until the logo appears, then release only power while still holding Volume Down."] },
  { maker: "Won't start at all", steps: ["Hold the power button for 10 to 20 seconds to force a restart, then try the steps for your phone above.", "Many phones can also start the recovery menu with the power and volume keys; look up your exact model, as keys differ."] },
];

export const RESCUE_NOTE = "Coolant only ever disables apps or removes them for your user, and keeps the files in the phone's system area. So a restore brings back apps you removed, and even a factory reset would bring back the original apps.";
