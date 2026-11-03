import type { AppInfo } from "./types";

export type InstallerKey = "any" | "preinstalled" | "play" | "maker-store" | "file" | "other-store";
export const INSTALLER_OPTIONS: { key: InstallerKey; label: string }[] = [
  { key: "any", label: "Installed by: anyone" },
  { key: "preinstalled", label: "Came with the phone" },
  { key: "play", label: "Google Play" },
  { key: "maker-store", label: "The maker's store" },
  { key: "file", label: "From a file" },
  { key: "other-store", label: "Another store" },
];

const MAKER_STORES = new Set(["com.sec.android.app.samsungapps", "com.xiaomi.mipicks", "com.xiaomi.market", "com.huawei.appmarket", "com.hihonor.appmarket", "com.heytap.market", "com.vivo.appstore", "com.oppo.market", "com.android.vending.billing.InAppBillingService.COIN"]);
const FILE_INSTALLERS = new Set(["com.android.packageinstaller", "com.google.android.packageinstaller", "com.miui.packageinstaller", "com.miui.global.packageinstaller"]);

/** Who installed an app, from what the helper reported; `null` when we do not know (no helper data). */
export function installerKey(app: Pick<AppInfo, "details">): Exclude<InstallerKey, "any"> | null {
  const d = app.details;
  if (!d) return null;
  const i = d.installer.trim();
  if (!i) return "preinstalled";
  if (i === "com.android.vending") return "play";
  if (MAKER_STORES.has(i)) return "maker-store";
  if (FILE_INSTALLERS.has(i)) return "file";
  return "other-store";
}

/** The app this one duplicates, when that app is also on the phone and switched on. */
export function duplicateTarget(app: Pick<AppInfo, "duplicateOf">, byName: ReadonlyMap<string, AppInfo>): AppInfo | null {
  if (!app.duplicateOf) return null;
  const t = byName.get(app.duplicateOf);
  return t && t.state === "enabled" ? t : null;
}

export interface Filters {
  category: string; // "any" or a category key
  installer: InstallerKey;
  duplicatesOnly: boolean;
}
export const NO_FILTERS: Filters = { category: "any", installer: "any", duplicatesOnly: false };

export function matchesFilters(app: AppInfo, f: Filters, byName: ReadonlyMap<string, AppInfo>): boolean {
  if (f.category !== "any" && app.category !== f.category) return false;
  if (f.installer !== "any" && installerKey(app) !== f.installer) return false;
  if (f.duplicatesOnly && !duplicateTarget(app, byName)) return false;
  return true;
}

/** Categories present in the list with how many apps each has, most common first. */
export function categoryCounts(apps: AppInfo[]): { key: string; count: number }[] {
  const m = new Map<string, number>();
  for (const a of apps) if (a.category) m.set(a.category, (m.get(a.category) ?? 0) + 1);
  return [...m].map(([key, count]) => ({ key, count })).sort((a, b) => b.count - a.count || a.key.localeCompare(b.key));
}
