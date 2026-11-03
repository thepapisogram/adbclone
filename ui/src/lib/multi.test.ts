import { describe, expect, it } from "vitest";
import { appliesTo, otherReady, repeatable } from "./multi";

describe("multi-phone helpers", () => {
  it("lists only other, ready phones", () => {
    const d = [{ serial: "A", state: "device" }, { serial: "B", state: "device" }, { serial: "C", state: "offline" }, { serial: "D", state: "unauthorized" }];
    expect(otherReady(d, "A").map((x) => x.serial)).toEqual(["B"]);
  });
  it("skips apps the other phone does not have or that are already done", () => {
    const theirs = [{ name: "a", state: "enabled" as const }, { name: "b", state: "disabled" as const }, { name: "c", state: "uninstalled" as const }];
    expect(appliesTo("disable", ["a", "b", "c", "z"], theirs)).toEqual({ names: ["a"], skipped: 3 });
    expect(appliesTo("restore", ["a", "c"], theirs)).toEqual({ names: ["c"], skipped: 1 });
    expect(appliesTo("uninstall", ["a", "b", "c"], theirs).names).toEqual(["a", "b"]);
  });
  it("only repeats removal-type actions", () => {
    expect(repeatable("disable")).toBe(true);
    expect(repeatable("stop")).toBe(false);
  });
});
