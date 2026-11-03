import { describe, expect, it } from "vitest";
import { PRESETS, SCALES, currentPreset, nearestScale, speedWord } from "./animations";

describe("animation presets", () => {
  it("only use offered speeds and have unique ids", () => {
    for (const p of PRESETS) for (const v of Object.values(p.mix)) expect(SCALES).toContain(v);
    expect(new Set(PRESETS.map((p) => p.id)).size).toBe(PRESETS.length);
  });
  it("recognises the current preset, treating unset as normal", () => {
    expect(currentPreset(null, undefined, "")?.id).toBe("normal");
    expect(currentPreset("0.5", "0.50", "0.5")?.id).toBe("snappy");
    expect(currentPreset("1.25", "1.0", "0.5")?.id).toBe("flow");
    expect(currentPreset("0.6", "0.5", "0.5")).toBeNull();
  });
  it("snaps odd values to an offered speed", () => {
    expect(nearestScale("0.6")).toBe("0.5");
    expect(nearestScale("10")).toBe("1.5");
    expect(nearestScale("abc")).toBe("1");
    expect(nearestScale(null)).toBe("1");
  });
  it("describes speeds in plain words", () => {
    expect(speedWord("1")).toBe("normal");
    expect(speedWord("0")).toBe("off");
    expect(speedWord("0.5")).toBe("2× faster");
    expect(speedWord("1.5")).toBe("1.5× slower");
  });
});
