import { describe, expect, it } from "vitest";
import { DEFAULT_SETTINGS, parseSettings } from "./settings";
import { reasonOf } from "./reason";

describe("parseSettings", () => {
  it("uses defaults for nothing, garbage or the wrong type", () => {
    expect(parseSettings(null)).toEqual(DEFAULT_SETTINGS);
    expect(parseSettings("")).toEqual(DEFAULT_SETTINGS);
    expect(parseSettings("not json")).toEqual(DEFAULT_SETTINGS);
    expect(parseSettings("[1,2]")).toEqual(DEFAULT_SETTINGS);
    expect(parseSettings('"text"')).toEqual(DEFAULT_SETTINGS);
  });
  it("keeps valid values and fixes only the invalid ones", () => {
    expect(parseSettings('{"theme":"dark","plain":false,"language":"es"}')).toEqual({ theme: "dark", plain: false, language: "es" });
    expect(parseSettings('{"theme":"neon","plain":"yes","language":"../x"}')).toEqual(DEFAULT_SETTINGS);
    expect(parseSettings('{"theme":"light"}')).toEqual({ ...DEFAULT_SETTINGS, theme: "light" });
  });
  it("plain-language mode is on by default", () => {
    expect(DEFAULT_SETTINGS.plain).toBe(true);
  });
});

describe("reasonOf", () => {
  it("prefers the description, then says when nothing is known", () => {
    expect(reasonOf({ description: " Voice assistant. ", risk: "recommended" })).toBe("Voice assistant.");
    expect(reasonOf({ description: null, risk: "unknown" })).toBe("No information about this app yet.");
    expect(reasonOf({ description: "", risk: "advanced" })).toBe("");
  });
});
