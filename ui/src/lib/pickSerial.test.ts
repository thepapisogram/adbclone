import { describe, expect, it } from "vitest";
import { pickSerial } from "./pickSerial";
import type { Device } from "./types";

const dev = (serial: string, state = "device"): Device => ({ serial, state });

describe("pickSerial", () => {
  it("keeps the selected phone while it is ready", () => {
    expect(pickSerial("A", [dev("A"), dev("B")], undefined, 2)).toEqual({ serial: "A", misses: 0 });
  });
  it("survives one or two polls where the phone is offline or missing", () => {
    let r = pickSerial("A", [dev("A", "offline"), dev("B")], undefined, 0);
    expect(r).toEqual({ serial: "A", misses: 1 });
    r = pickSerial("A", [dev("B")], undefined, r.misses);
    expect(r).toEqual({ serial: "A", misses: 2 });
  });
  it("gives up after three misses in a row and moves to another ready phone", () => {
    expect(pickSerial("A", [dev("B")], undefined, 2)).toEqual({ serial: "B", misses: 0 });
    expect(pickSerial("A", [], undefined, 2)).toEqual({ serial: "", misses: 0 });
  });
  it("a recovered phone resets the counter", () => {
    expect(pickSerial("A", [dev("A")], undefined, 2).misses).toBe(0);
  });
  it("selects a just-connected phone when asked, and picks the first ready one when nothing is selected", () => {
    expect(pickSerial("A", [dev("A"), dev("C")], "C", 0).serial).toBe("C");
    expect(pickSerial("A", [dev("A")], "C", 0).serial, "not ready yet: keep A").toBe("A");
    expect(pickSerial("", [dev("X", "unauthorized"), dev("Y")], undefined, 0).serial).toBe("Y");
  });
});
