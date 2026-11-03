import { describe, expect, it } from "vitest";
import type { AppDetails, AppInfo } from "./types";
import { categoryCounts, duplicateTarget, installerKey, matchesFilters, NO_FILTERS } from "./filters";

const details = (installer: string): AppDetails => ({ launchable: true, installer, versionName: "1", versionCode: 1, firstInstall: 0, lastUpdate: 0, targetSdk: 34, updatedSystem: false });
const app = (over: Partial<AppInfo>): AppInfo => ({ name: "a.b", label: "A", system: true, state: "enabled", risk: "recommended", disableSafety: "safe", uninstallSafety: "safe", tier: "safe", confidence: "high", ...over });
const by = (...apps: AppInfo[]) => new Map(apps.map((a) => [a.name, a]));

describe("installerKey", () => {
  it("says nothing without helper data", () => expect(installerKey(app({}))).toBeNull());
  it("recognises the common installers", () => {
    expect(installerKey(app({ details: details("") }))).toBe("preinstalled");
    expect(installerKey(app({ details: details("com.android.vending") }))).toBe("play");
    expect(installerKey(app({ details: details("com.sec.android.app.samsungapps") }))).toBe("maker-store");
    expect(installerKey(app({ details: details("com.xiaomi.mipicks") }))).toBe("maker-store");
    expect(installerKey(app({ details: details("com.google.android.packageinstaller") }))).toBe("file");
    expect(installerKey(app({ details: details("org.fdroid.fdroid") }))).toBe("other-store");
  });
});

describe("duplicateTarget", () => {
  const chrome = app({ name: "com.android.chrome", label: "Chrome" });
  it("finds the app this one duplicates when it is on the phone and enabled", () => {
    const gal = app({ name: "x.browser", duplicateOf: "com.android.chrome" });
    expect(duplicateTarget(gal, by(chrome, gal))?.label).toBe("Chrome");
  });
  it("ignores it when the other app is missing or switched off, or there is no link", () => {
    const gal = app({ name: "x.browser", duplicateOf: "com.android.chrome" });
    expect(duplicateTarget(gal, by(gal))).toBeNull();
    expect(duplicateTarget(gal, by({ ...chrome, state: "disabled" }, gal))).toBeNull();
    expect(duplicateTarget(app({}), by(chrome))).toBeNull();
  });
});

describe("matchesFilters", () => {
  const chrome = app({ name: "com.android.chrome" });
  const dup = app({ name: "x.browser", category: "vendor-app", duplicateOf: "com.android.chrome", details: details("") });
  const m = by(chrome, dup);
  it("shows everything with no filters", () => expect(matchesFilters(dup, NO_FILTERS, m)).toBe(true));
  it("filters by category, installer and duplicates, all together", () => {
    expect(matchesFilters(dup, { ...NO_FILTERS, category: "vendor-app" }, m)).toBe(true);
    expect(matchesFilters(dup, { ...NO_FILTERS, category: "google-app" }, m)).toBe(false);
    expect(matchesFilters(dup, { ...NO_FILTERS, installer: "preinstalled" }, m)).toBe(true);
    expect(matchesFilters(dup, { ...NO_FILTERS, installer: "play" }, m)).toBe(false);
    expect(matchesFilters(chrome, { ...NO_FILTERS, duplicatesOnly: true }, m)).toBe(false);
    expect(matchesFilters(dup, { category: "vendor-app", installer: "preinstalled", duplicatesOnly: true }, m)).toBe(true);
  });
  it("an installer filter hides apps whose installer is unknown", () => {
    expect(matchesFilters(chrome, { ...NO_FILTERS, installer: "preinstalled" }, m)).toBe(false);
  });
});

describe("categoryCounts", () => {
  it("counts and orders categories, skipping apps without one", () => {
    const c = categoryCounts([app({ category: "b" }), app({ category: "a" }), app({ category: "b" }), app({ category: null })]);
    expect(c).toEqual([{ key: "b", count: 2 }, { key: "a", count: 1 }]);
  });
});
