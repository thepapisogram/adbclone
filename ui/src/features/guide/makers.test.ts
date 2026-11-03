import { describe, expect, it } from "vitest";
import { MAKERS, makerFor } from "./makers";

describe("makerFor", () => {
  it("recognises brands in any case and sub-brands", () => {
    expect(makerFor("samsung")).toBe("samsung");
    expect(makerFor("Xiaomi")).toBe("xiaomi");
    expect(makerFor("POCO")).toBe("xiaomi");
    expect(makerFor("realme")).toBe("oppo");
    expect(makerFor("HONOR")).toBe("huawei");
    expect(makerFor("Google")).toBe("google");
    expect(makerFor("iQOO")).toBe("vivo");
    expect(makerFor("motorola")).toBe("motorola");
  });
  it("falls back to generic help for unknown or missing makers", () => {
    expect(makerFor("Fairphone")).toBe("other");
    expect(makerFor("")).toBe("other");
    expect(makerFor(null)).toBe("other");
    expect(makerFor(undefined)).toBe("other");
  });
  it("every maker has a build-number hint and at least one note, and the keys are unique", () => {
    expect(new Set(MAKERS.map((m) => m.key)).size).toBe(MAKERS.length);
    for (const m of MAKERS) {
      expect(m.buildNumber.length).toBeGreaterThan(10);
      expect(m.notes.length).toBeGreaterThan(0);
    }
  });
  it("Xiaomi's extra security toggle is mentioned", () => {
    expect(MAKERS.find((m) => m.key === "xiaomi")!.notes.join(" ")).toContain("USB debugging (Security settings)");
  });
});
