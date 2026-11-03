import type { AppInfo } from "../../lib/types";

export type Preset = "safe" | "balanced" | "aggressive";
/** Where an app is listed in the wizard. `unrated` means preinstalled but nobody has rated it. */
export type Group = "safe" | "balanced" | "aggressive" | "unrated";

export interface Context {
  /** Also offer apps the person installed themselves. */
  includeUser: boolean;
  /** Apps the phone uses for its main jobs right now (home screen, dialer, browser, keyboard...). */
  protectedPkgs: ReadonlySet<string>;
}

/**
 * The tier an app is listed under. A rating we are less sure about moves one step toward "more careful":
 * a safe-looking app we are only fairly sure of is listed under balanced, and so on.
 */
export function effectiveTier(app: Pick<AppInfo, "tier" | "confidence">): "safe" | "balanced" | "aggressive" | null {
  const { tier, confidence } = app;
  if (tier === "safe") return confidence === "high" ? "safe" : confidence === "medium" ? "balanced" : "aggressive";
  if (tier === "balanced") return confidence === "low" ? "aggressive" : "balanced";
  if (tier === "aggressive") return "aggressive";
  return null; // never, or no rating
}

/**
 * Which group an app belongs to in the wizard, or null if it must never be offered.
 * Only enabled apps are offered; never anything rated unsafe or in use as a default app; and, unless asked,
 * only preinstalled system apps.
 */
export function groupOf(app: AppInfo, ctx: Context): Group | null {
  if (app.state !== "enabled") return null;
  if (!app.system && !ctx.includeUser) return null;
  if (ctx.protectedPkgs.has(app.name)) return null;
  if (app.risk === "unsafe" || app.disableSafety === "unsafe" || app.uninstallSafety === "unsafe" || app.tier === "never") return null;
  if (app.tier === "unrated" || app.risk === "unknown") return app.system ? "unrated" : null; // unrated apps you installed yourself are not bloat
  return effectiveTier(app);
}

/** Groups shown for each preset. */
export function visibleGroups(preset: Preset): Group[] {
  return preset === "safe" ? ["safe"] : preset === "balanced" ? ["safe", "balanced"] : ["safe", "balanced", "aggressive", "unrated"];
}

export function candidates(apps: AppInfo[], preset: Preset, ctx: Context): { app: AppInfo; group: Group }[] {
  const show = visibleGroups(preset);
  const out: { app: AppInfo; group: Group }[] = [];
  for (const app of apps) {
    const group = groupOf(app, ctx);
    if (group && show.includes(group)) out.push({ app, group });
  }
  return out;
}

/** Preselected apps: only those in the safe group (which already requires high confidence). Everything else is the user's choice. */
export function defaultSelection(apps: AppInfo[], preset: Preset, ctx: Context): Set<string> {
  return new Set(candidates(apps, preset, ctx).filter((c) => c.group === "safe").map((c) => c.app.name));
}

/** Apps that would have been offered but are protected because the phone uses them for a main job. */
export function protectedOffers(apps: AppInfo[], ctx: Context, reasons: ReadonlyMap<string, string>): { app: AppInfo; reason: string }[] {
  const out: { app: AppInfo; reason: string }[] = [];
  for (const app of apps) {
    const reason = reasons.get(app.name);
    if (!reason || app.state !== "enabled") continue;
    if (!app.system && !ctx.includeUser) continue;
    out.push({ app, reason });
  }
  return out;
}

/** RAM (KiB) currently used by the selected apps, from a running sample. */
export function ramTotalKb(selected: Set<string>, ramByName: Map<string, number>): number {
  let total = 0;
  for (const n of selected) total += ramByName.get(n) ?? 0;
  return total;
}
