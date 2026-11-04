import { beforeEach, describe, expect, it } from "vitest";
import { applyCleared, clearedCount, forgetCleared, rememberCleared, fmtFreed } from "./cleared";

const s = (package_: string, cacheBytes: number) => ({ package: package_, appBytes: 100, dataBytes: 50, cacheBytes });
beforeEach(() => forgetCleared("A"));

describe("cleared caches", () => {
  it("shows apps as cleared while the phone still reports the old figure", () => {
    const old = [s("a", 500), s("b", 0), s("c", 200)];
    expect(rememberCleared("A", old)).toBe(700);
    const shown = applyCleared("A", old);
    expect(shown.map((x) => x.cacheBytes)).toEqual([0, 0, 0]);
    expect(clearedCount("A", old)).toBe(2);
  });
  it("trusts the phone once it reports something new", () => {
    rememberCleared("A", [s("a", 500), s("c", 200)]);
    const fresh = [s("a", 30), s("c", 200)];
    expect(applyCleared("A", fresh).map((x) => x.cacheBytes)).toEqual([30, 0]);
  });
  it("only affects the phone that was cleaned, and totals add up across two cleanups", () => {
    rememberCleared("A", [s("a", 500)]);
    expect(applyCleared("B", [s("a", 500)])[0].cacheBytes).toBe(500);
    rememberCleared("A", [s("z", 100)]);
    expect(applyCleared("A", [s("a", 500), s("z", 100)]).map((x) => x.cacheBytes)).toEqual([0, 0]);
  });
  it("leaves figures alone when nothing was cleared", () => {
    const x = [s("a", 5)];
    expect(applyCleared("A", x)).toBe(x);
  });
  it("formats the amount freed", () => {
    expect(fmtFreed(3_221_225_472)).toBe("3.00 GB");
    expect(fmtFreed(5 * 1_048_576)).toBe("5 MB");
    expect(fmtFreed(100)).toBe("1 KB");
  });
});
