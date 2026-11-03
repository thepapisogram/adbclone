import { describe, expect, it } from "vitest";
import { drainPerHour, isWarm, type Entry } from "./history";

const e = (t: number, level: number | null, charging: boolean | null): Entry => ({ t, label: "x", hot: null, batt: null, level, charging, availKb: null });
describe("history maths", () => {
  it("computes drain per hour", () => { expect(drainPerHour(e(0, 80, false), e(3_600_000, 74, false))).toBeCloseTo(6); });
  it("refuses unfair comparisons", () => {
    expect(drainPerHour(e(0, 80, true), e(3_600_000, 74, false))).toBeNull();
    expect(drainPerHour(e(0, 80, false), e(60_000, 79, false))).toBeNull();
    expect(drainPerHour(e(0, 70, false), e(3_600_000, 75, false))).toBeNull();
    expect(drainPerHour(e(0, null, false), e(3_600_000, 75, false))).toBeNull();
  });
  it("flags a warm phone", () => {
    expect(isWarm(43, null, 0)).toBe(true);
    expect(isWarm(null, 40, null)).toBe(true);
    expect(isWarm(30, 31, 2)).toBe(true);
    expect(isWarm(35, 33, 0)).toBe(false);
    expect(isWarm(null, null, null)).toBe(false);
  });
});
