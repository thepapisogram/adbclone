import type { AppInfo, Usage } from "../../lib/types";

export interface UnusedRow {
  app: AppInfo;
  /** Days since it was last opened, or `null` when the phone has no record of it being opened. */
  daysAgo: number | null;
}

/**
 * Apps worth a second look because they have not been opened for `days` days (or the phone has no record of
 * them being opened at all). Never includes apps rated unsafe, apps the phone uses for a main job,
 * apps that are switched off already, or (unless asked) preinstalled apps. When the phone told us which apps
 * have a launcher icon, background-only apps are left out: they are not something you "open".
 */
export function unusedApps(
  apps: AppInfo[],
  usage: ReadonlyMap<string, number>,
  opts: { days: number; includeSystem: boolean; protectedPkgs: ReadonlySet<string> },
): UnusedRow[] {
  const rows: UnusedRow[] = [];
  for (const app of apps) {
    if (app.state !== "enabled") continue;
    if (app.system && !opts.includeSystem) continue;
    if (opts.protectedPkgs.has(app.name)) continue;
    if (app.risk === "unsafe" || app.tier === "never" || app.disableSafety === "unsafe") continue;
    if (app.details && !app.details.launchable) continue;
    const d = usage.get(app.name);
    if (d !== undefined && d < opts.days) continue;
    rows.push({ app, daysAgo: d ?? null });
  }
  // No record first (most doubtful), then longest unused.
  return rows.sort((a, b) => (b.daysAgo ?? Infinity) - (a.daysAgo ?? Infinity) || a.app.label.localeCompare(b.app.label));
}

export function usageMap(entries: Usage[]): Map<string, number> {
  return new Map(entries.map((e) => [e.package, e.daysAgo]));
}
