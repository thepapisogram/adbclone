import { describe, expect, it } from "vitest";
import type { AppInfo } from "../../lib/types";
import { unusedApps, usageMap } from "./unused";

const app = (over: Partial<AppInfo>): AppInfo => ({ name: "a.b", label: "A", system: false, state: "enabled", risk: "unknown", disableSafety: "unknown", uninstallSafety: "unknown", tier: "unrated", confidence: "low", ...over });
const base = { days: 30, includeSystem: false, protectedPkgs: new Set<string>() };

describe("unusedApps", () => {
  const apps = [app({ name: "recent" }), app({ name: "old" }), app({ name: "norecord" }), app({ name: "system.old", system: true })];
  const usage = usageMap([{ package: "recent", daysAgo: 3, lastUsed: "x" }, { package: "old", daysAgo: 90, lastUsed: "x" }, { package: "system.old", daysAgo: 400, lastUsed: "x" }]);
  it("lists apps past the limit, and apps with no record, with the doubtful ones first", () => {
    expect(unusedApps(apps, usage, base).map((r) => [r.app.name, r.daysAgo])).toEqual([["norecord", null], ["old", 90]]);
  });
  it("counts the limit exactly: opened 30 days ago is unused at 30, not at 31", () => {
    const u = usageMap([{ package: "old", daysAgo: 30, lastUsed: "x" }]);
    expect(unusedApps([app({ name: "old" })], u, base)).toHaveLength(1);
    expect(unusedApps([app({ name: "old" })], u, { ...base, days: 31 })).toHaveLength(0);
  });
  it("includes preinstalled apps only when asked", () => {
    expect(unusedApps(apps, usage, { ...base, includeSystem: true }).map((r) => r.app.name)).toContain("system.old");
  });
  it("never offers unsafe, protected or already-disabled apps", () => {
    const list = [app({ name: "u", risk: "unsafe", tier: "never", disableSafety: "unsafe" }), app({ name: "p" }), app({ name: "d", state: "disabled" }), app({ name: "ok" })];
    expect(unusedApps(list, new Map(), { ...base, protectedPkgs: new Set(["p"]) }).map((r) => r.app.name)).toEqual(["ok"]);
  });
  it("skips background-only apps when the phone reported launcher icons", () => {
    const d = { launchable: false, installer: "", versionName: "", versionCode: 1, firstInstall: 0, lastUpdate: 0, targetSdk: 34, updatedSystem: false };
    expect(unusedApps([app({ name: "bg", details: d }), app({ name: "fg", details: { ...d, launchable: true } })], new Map(), base).map((r) => r.app.name)).toEqual(["fg"]);
  });
});
