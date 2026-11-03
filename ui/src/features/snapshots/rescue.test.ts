import { describe, expect, it } from "vitest";
import { latestSnapshot, SAFE_MODE } from "./rescue";
import type { SnapshotMeta } from "../../lib/types";

const m = (id: string, created: number): SnapshotMeta => ({ id, name: id, created, auto: true, enabled: 1, disabled: 0, uninstalled: 0 });
describe("rescue", () => {
  it("offers the newest snapshot", () => {
    expect(latestSnapshot([m("a", 5), m("c", 30), m("b", 10)])?.id).toBe("c");
    expect(latestSnapshot([])).toBeNull();
  });
  it("has numbered steps for every maker", () => {
    for (const s of SAFE_MODE) expect(s.steps.length).toBeGreaterThan(0);
  });
});
