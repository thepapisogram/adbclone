/** Animation speeds: 1 = normal, lower = faster. Only these values are ever sent to the phone. */
export const SCALES = ["0", "0.25", "0.5", "0.75", "1", "1.25", "1.5"] as const;
export type Scale = (typeof SCALES)[number];
export interface Mix { window: Scale; transition: Scale; animator: Scale }
export interface AnimPreset { id: string; title: string; text: string; mix: Mix }

export const PRESETS: AnimPreset[] = [
  { id: "normal", title: "Normal", text: "The phone's default speed.", mix: { window: "1", transition: "1", animator: "1" } },
  { id: "quick", title: "Quick and smooth", text: "Best all-round choice: windows stay readable while screens and in-app motion finish sooner.", mix: { window: "0.75", transition: "0.5", animator: "0.5" } },
  { id: "snappy", title: "Snappy", text: "Everything at half time. Feels fast and gives the graphics chip less to do.", mix: { window: "0.5", transition: "0.5", animator: "0.5" } },
  { id: "fast", title: "Very fast", text: "A quarter of the time. Quick, but motion can look abrupt.", mix: { window: "0.25", transition: "0.25", animator: "0.25" } },
  { id: "instant", title: "Instant screens", text: "Windows and screens switch with no animation. In-app motion stays at half time, because some apps misbehave when it is fully off.", mix: { window: "0", transition: "0", animator: "0.5" } },
  { id: "flow", title: "Calm window, fast apps", text: "A slightly slower window animation with quicker in-app motion: a distinct, relaxed look that still feels responsive.", mix: { window: "1.25", transition: "1", animator: "0.5" } },
];

const num = (v?: string | null) => (v == null || v === "" ? 1 : Number(v));
const same = (a?: string | null, b?: string) => Math.abs(num(a) - Number(b)) < 1e-6;

/** The preset matching the phone's current three values, if any. */
export function currentPreset(w?: string | null, t?: string | null, a?: string | null): AnimPreset | null {
  return PRESETS.find((p) => same(w, p.mix.window) && same(t, p.mix.transition) && same(a, p.mix.animator)) ?? null;
}

/** Nearest offered speed for a value read from the phone (so the pickers always show something valid). */
export function nearestScale(v?: string | null): Scale {
  const n = num(v);
  return SCALES.reduce((best, s) => (Math.abs(Number(s) - n) < Math.abs(Number(best) - n) ? s : best), SCALES[4]);
}

export const speedWord = (s: Scale) => (s === "0" ? "off" : s === "1" ? "normal" : Number(s) < 1 ? `${Math.round(1 / Number(s) * 10) / 10}× faster` : `${Number(s)}× slower`);
