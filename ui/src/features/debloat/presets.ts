import type { AppInfo } from "../../lib/types";

export type Preset = "safe" | "balanced" | "aggressive";
/** safe: rated safe to remove. advanced: rated "caution". unrated: preinstalled but nobody has rated it. */
export type Group = "safe" | "advanced" | "unrated";

/**
 * Which group an app belongs to in the wizard, or null if it must never be offered.
 * Only enabled apps are offered, never anything rated unsafe, and (unless asked) only preinstalled system apps.
 */
export function groupOf(app: AppInfo, includeUser: boolean): Group | null {
  if (app.state !== "enabled") return null;
  if (!app.system && !includeUser) return null;
  if (app.risk === "unsafe" || app.disableSafety === "unsafe" || app.uninstallSafety === "unsafe") return null;
  if (app.risk === "unknown") return app.system ? "unrated" : null; // unrated apps you installed yourself are not bloat
  if (app.risk === "recommended" && app.disableSafety === "safe") return "safe";
  return "advanced"; // advanced, or recommended-but-overridden to caution
}

/** Groups shown for each preset. */
export function visibleGroups(preset: Preset): Group[] {
  return preset === "safe" ? ["safe"] : preset === "balanced" ? ["safe", "advanced"] : ["safe", "advanced", "unrated"];
}

export function candidates(apps: AppInfo[], preset: Preset, includeUser: boolean): { app: AppInfo; group: Group }[] {
  const show = visibleGroups(preset);
  const out: { app: AppInfo; group: Group }[] = [];
  for (const app of apps) {
    const group = groupOf(app, includeUser);
    if (group && show.includes(group)) out.push({ app, group });
  }
  return out;
}

/** Preselected apps: only those rated safe. Advanced and unrated items are always the user's choice. */
export function defaultSelection(apps: AppInfo[], preset: Preset, includeUser: boolean): Set<string> {
  return new Set(candidates(apps, preset, includeUser).filter((c) => c.group === "safe").map((c) => c.app.name));
}

/** RAM (KiB) currently used by the selected apps, from a running sample. */
export function ramTotalKb(selected: Set<string>, ramByName: Map<string, number>): number {
  let total = 0;
  for (const n of selected) total += ramByName.get(n) ?? 0;
  return total;
}
