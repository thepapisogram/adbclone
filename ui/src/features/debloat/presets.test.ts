import { describe, expect, it } from "vitest";
import type { AppInfo } from "../../lib/types";
import { candidates, defaultSelection, effectiveTier, groupOf, protectedOffers, ramTotalKb, visibleGroups, type Context } from "./presets";

const app = (over: Partial<AppInfo>): AppInfo => ({
  name: "com.x", label: "X", system: true, state: "enabled", risk: "recommended",
  disableSafety: "safe", uninstallSafety: "safe", tier: "safe", confidence: "high", ...over,
});
const ctx = (over: Partial<Context> = {}): Context => ({ includeUser: false, protectedPkgs: new Set(), ...over });
const unsafe = { risk: "unsafe", disableSafety: "unsafe", uninstallSafety: "unsafe", tier: "never" } as const;
const unrated = { risk: "unknown", disableSafety: "unknown", uninstallSafety: "unknown", tier: "unrated", confidence: "low" } as const;

describe("effectiveTier", () => {
  it("keeps a confident rating as it is", () => {
    expect(effectiveTier({ tier: "safe", confidence: "high" })).toBe("safe");
    expect(effectiveTier({ tier: "balanced", confidence: "high" })).toBe("balanced");
    expect(effectiveTier({ tier: "aggressive", confidence: "high" })).toBe("aggressive");
  });
  it("moves a less certain rating toward more careful, never the other way", () => {
    expect(effectiveTier({ tier: "safe", confidence: "medium" })).toBe("balanced");
    expect(effectiveTier({ tier: "safe", confidence: "low" })).toBe("aggressive");
    expect(effectiveTier({ tier: "balanced", confidence: "medium" })).toBe("balanced");
    expect(effectiveTier({ tier: "balanced", confidence: "low" })).toBe("aggressive");
    expect(effectiveTier({ tier: "aggressive", confidence: "low" })).toBe("aggressive");
  });
  it("never offers 'never' or unrated", () => {
    expect(effectiveTier({ tier: "never", confidence: "high" })).toBeNull();
    expect(effectiveTier({ tier: "unrated", confidence: "high" })).toBeNull();
  });
});

describe("groupOf", () => {
  it("lists rated system apps under their effective tier", () => {
    expect(groupOf(app({}), ctx())).toBe("safe");
    expect(groupOf(app({ tier: "safe", confidence: "medium" }), ctx())).toBe("balanced");
    expect(groupOf(app({ tier: "aggressive", risk: "advanced" }), ctx())).toBe("aggressive");
    expect(groupOf(app({ ...unrated }), ctx())).toBe("unrated");
  });
  it("never offers unsafe apps, whichever field says so", () => {
    expect(groupOf(app({ ...unsafe }), ctx({ includeUser: true }))).toBeNull();
    expect(groupOf(app({ disableSafety: "unsafe" }), ctx())).toBeNull();
    expect(groupOf(app({ uninstallSafety: "unsafe" }), ctx())).toBeNull();
    expect(groupOf(app({ tier: "never" }), ctx())).toBeNull();
  });
  it("never offers an app the phone is using for a main job", () => {
    expect(groupOf(app({ name: "com.android.chrome" }), ctx({ protectedPkgs: new Set(["com.android.chrome"]) }))).toBeNull();
    expect(groupOf(app({ name: "com.other" }), ctx({ protectedPkgs: new Set(["com.android.chrome"]) }))).toBe("safe");
  });
  it("skips apps that are not enabled", () => {
    expect(groupOf(app({ state: "disabled" }), ctx())).toBeNull();
    expect(groupOf(app({ state: "uninstalled" }), ctx())).toBeNull();
  });
  it("offers user-installed apps only when asked, and never unrated ones", () => {
    expect(groupOf(app({ system: false }), ctx())).toBeNull();
    expect(groupOf(app({ system: false }), ctx({ includeUser: true }))).toBe("safe");
    expect(groupOf(app({ system: false, ...unrated }), ctx({ includeUser: true }))).toBeNull();
  });
});

describe("presets", () => {
  const apps = [
    app({ name: "a.safe" }),
    app({ name: "b.bal", tier: "balanced", risk: "advanced", disableSafety: "caution", uninstallSafety: "caution" }),
    app({ name: "c.agg", tier: "aggressive", risk: "advanced", disableSafety: "caution", uninstallSafety: "caution" }),
    app({ name: "d.unrated", ...unrated }),
    app({ name: "e.unsafe", ...unsafe }),
    app({ name: "f.safe-medium", confidence: "medium" }),
  ];
  it("shows more groups as the preset gets stronger", () => {
    expect(visibleGroups("safe")).toEqual(["safe"]);
    const names = (p: "safe" | "balanced" | "aggressive") => candidates(apps, p, ctx()).map((c) => c.app.name);
    expect(names("safe")).toEqual(["a.safe"]);
    expect(names("balanced")).toEqual(["a.safe", "b.bal", "f.safe-medium"]);
    expect(names("aggressive")).toEqual(["a.safe", "b.bal", "c.agg", "d.unrated", "f.safe-medium"]);
  });
  it("only preselects confident safe apps, in every preset", () => {
    for (const p of ["safe", "balanced", "aggressive"] as const) {
      expect([...defaultSelection(apps, p, ctx())]).toEqual(["a.safe"]);
    }
  });
  it("lists protected apps separately instead of offering them", () => {
    const c = ctx({ protectedPkgs: new Set(["a.safe"]) });
    expect(candidates(apps, "safe", c)).toEqual([]);
    const out = protectedOffers(apps, c, new Map([["a.safe", "your browser"], ["gone", "your keyboard"]]));
    expect(out.map((o) => [o.app.name, o.reason])).toEqual([["a.safe", "your browser"]]);
  });
  it("sums RAM for the selection, ignoring apps that are not running", () => {
    const ram = new Map([["a.safe", 1000], ["b.bal", 500]]);
    expect(ramTotalKb(new Set(["a.safe", "d.unrated"]), ram)).toBe(1000);
    expect(ramTotalKb(new Set(), ram)).toBe(0);
  });
});
