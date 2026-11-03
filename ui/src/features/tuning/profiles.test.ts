import { describe, expect, it } from "vitest";
import { PROFILES, profileOps } from "./profiles";

describe("profiles", () => {
  it("gaming removes our limits and adds none", () => {
    const ops = profileOps(PROFILES[0], true);
    expect(ops.every((o) => o.kind === "toggle" && !o.on)).toBe(true);
    expect(ops).toHaveLength(3);
  });
  it("battery saver turns saver and the cap on", () => {
    const ops = profileOps(PROFILES[1], true);
    expect(ops).toContainEqual({ kind: "toggle", id: "battery_saver", on: true });
    expect(ops).toContainEqual({ kind: "toggle", id: "refresh", on: true });
    expect(ops[0].kind).toBe("animations");
  });
  it("skips the refresh cap on a 60 Hz screen", () => {
    expect(profileOps(PROFILES[2], false).some((o) => o.kind === "toggle" && o.id === "refresh")).toBe(false);
  });
});
