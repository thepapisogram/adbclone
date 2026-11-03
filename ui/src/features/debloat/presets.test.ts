import { describe, expect, it } from "vitest";
import type { AppInfo } from "../../lib/types";
import { candidates, defaultSelection, groupOf, ramTotalKb, visibleGroups } from "./presets";

const app = (over: Partial<AppInfo>): AppInfo => ({
  name: "com.x", label: "X", system: true, state: "enabled", risk: "recommended",
  disableSafety: "safe", uninstallSafety: "safe", ...over,
});

describe("groupOf", () => {
  it("classifies rated system apps", () => {
    expect(groupOf(app({}), false)).toBe("safe");
    expect(groupOf(app({ risk: "advanced", disableSafety: "caution", uninstallSafety: "caution" }), false)).toBe("advanced");
    expect(groupOf(app({ risk: "unknown", disableSafety: "unknown", uninstallSafety: "unknown" }), false)).toBe("unrated");
  });
  it("never offers unsafe apps, whichever field says so", () => {
    expect(groupOf(app({ risk: "unsafe", disableSafety: "unsafe", uninstallSafety: "unsafe" }), true)).toBeNull();
    expect(groupOf(app({ disableSafety: "unsafe" }), true)).toBeNull();
    expect(groupOf(app({ uninstallSafety: "unsafe" }), true)).toBeNull();
  });
  it("skips apps that are not enabled", () => {
    expect(groupOf(app({ state: "disabled" }), false)).toBeNull();
    expect(groupOf(app({ state: "uninstalled" }), false)).toBeNull();
  });
  it("only offers user-installed apps when asked, and never unrated ones", () => {
    expect(groupOf(app({ system: false }), false)).toBeNull();
    expect(groupOf(app({ system: false }), true)).toBe("safe");
    expect(groupOf(app({ system: false, risk: "unknown", disableSafety: "unknown", uninstallSafety: "unknown" }), true)).toBeNull();
  });
  it("treats a recommended app overridden to caution as advanced", () => {
    expect(groupOf(app({ disableSafety: "caution", uninstallSafety: "caution" }), false)).toBe("advanced");
  });
});

describe("presets", () => {
  const apps = [
    app({ name: "a.safe" }),
    app({ name: "b.adv", risk: "advanced", disableSafety: "caution", uninstallSafety: "caution" }),
    app({ name: "c.unrated", risk: "unknown", disableSafety: "unknown", uninstallSafety: "unknown" }),
    app({ name: "d.unsafe", risk: "unsafe", disableSafety: "unsafe", uninstallSafety: "unsafe" }),
  ];
  it("shows more groups as the preset gets stronger", () => {
    expect(visibleGroups("safe")).toEqual(["safe"]);
    expect(candidates(apps, "safe", false).map((c) => c.app.name)).toEqual(["a.safe"]);
    expect(candidates(apps, "balanced", false).map((c) => c.app.name)).toEqual(["a.safe", "b.adv"]);
    expect(candidates(apps, "aggressive", false).map((c) => c.app.name)).toEqual(["a.safe", "b.adv", "c.unrated"]);
  });
  it("only preselects apps rated safe, in every preset", () => {
    for (const p of ["safe", "balanced", "aggressive"] as const) {
      expect([...defaultSelection(apps, p, false)]).toEqual(["a.safe"]);
    }
  });
  it("sums RAM for the selection, ignoring apps that are not running", () => {
    const ram = new Map([["a.safe", 1000], ["b.adv", 500]]);
    expect(ramTotalKb(new Set(["a.safe", "c.unrated"]), ram)).toBe(1000);
    expect(ramTotalKb(new Set(), ram)).toBe(0);
  });
});
