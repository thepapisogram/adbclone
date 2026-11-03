import { useCallback } from "react";
import { useSettings } from "../settings";
import { en, type Key } from "./en";
import { es } from "./es";
import { fr } from "./fr";
import { pt } from "./pt";
import { de } from "./de";

export type { Key };
export const LANGUAGES: { code: string; name: string }[] = [
  { code: "en", name: "English" }, { code: "es", name: "Español" }, { code: "fr", name: "Français" }, { code: "pt", name: "Português" }, { code: "de", name: "Deutsch" },
];
const TABLES: Record<string, Partial<Record<Key, string>>> = { en, es, fr, pt, de };

/** Translate a key. Falls back to English for a missing translation, and fills `{name}` placeholders. */
export function translate(lang: string, key: Key, vars?: Record<string, string | number>): string {
  const base = lang.split("-")[0].toLowerCase();
  const text = TABLES[base]?.[key] ?? en[key];
  return vars ? text.replace(/\{(\w+)\}/g, (m, k) => (k in vars ? String(vars[k]) : m)) : text;
}

export function useT() {
  const { language } = useSettings();
  return useCallback((key: Key, vars?: Record<string, string | number>) => translate(language, key, vars), [language]);
}
export const TRANSLATIONS = TABLES;
