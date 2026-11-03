import { describe, expect, it } from "vitest";
import { en } from "./en";
import { LANGUAGES, TRANSLATIONS, translate, type Key } from "./index";

const placeholders = (s: string) => (s.match(/\{\w+\}/g) ?? []).sort().join(",");

describe("translations", () => {
  it("every language is registered", () => { for (const l of LANGUAGES) expect(TRANSLATIONS[l.code]).toBeDefined(); });
  for (const [code, table] of Object.entries(TRANSLATIONS)) {
    if (code === "en") continue;
    it(`${code}: only real keys, no empty text, same placeholders`, () => {
      for (const [k, v] of Object.entries(table)) {
        expect(k in en, `${code} has unknown key ${k}`).toBe(true);
        expect(v!.trim().length, `${code}.${k} is empty`).toBeGreaterThan(0);
        expect(placeholders(v!), `${code}.${k} placeholders`).toBe(placeholders(en[k as Key]));
      }
    });
    it(`${code}: translates every key (so nothing silently stays English)`, () => {
      const missing = (Object.keys(en) as Key[]).filter((k) => !(k in table));
      expect(missing).toEqual([]);
    });
  }
  it("falls back to English and fills placeholders", () => {
    expect(translate("xx", "tab.apps")).toBe("Apps");
    expect(translate("fr-CA", "action.disable")).toBe("Désactiver");
    expect(translate("en", "confirm.many", { action: "Disable", n: 3 })).toBe("Disable 3 apps?");
    expect(translate("en", "confirm.one", { action: "Disable", n: 1 })).toBe("Disable 1 app?");
  });
});
