import { describe, expect, it } from "vitest";
import { describePermissions, whatItCanDo } from "./permissions";

const p = (n: string, g = true) => ({ name: n, granted: g });
describe("permission wording", () => {
  it("translates known permissions, sensitive first, and counts the rest", () => {
    const d = describePermissions([p("android.permission.INTERNET"), p("android.permission.CAMERA"), p("com.vendor.permission.X"), p("android.permission.CAMERA"), p("android.permission.WEIRD_ONE")]);
    expect(d.known.map((k) => k.label)).toEqual(["Use the camera", "Use the internet"]);
    expect(d.otherCount).toBe(2);
  });
  it("only claims what is granted", () => {
    expect(whatItCanDo([p("android.permission.CAMERA", false), p("android.permission.INTERNET")])).toBe("");
    expect(whatItCanDo([p("android.permission.CAMERA"), p("android.permission.READ_CONTACTS")])).toBe("Allowed right now to: read your contacts, use the camera.");
  });
  it("shortens long lists", () => {
    const many = ["CAMERA", "RECORD_AUDIO", "READ_CONTACTS", "READ_SMS", "CALL_PHONE", "READ_CALENDAR"].map((n) => p(`android.permission.${n}`));
    expect(whatItCanDo(many)).toMatch(/and 2 more\.$/);
  });
  it("handles an empty list", () => {
    expect(describePermissions([])).toEqual({ known: [], otherCount: 0 });
  });
});
