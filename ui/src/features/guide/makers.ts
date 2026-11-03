/** Maker-specific help for turning on USB debugging. Menu names vary by model and Android version: this is guidance, not a promise. */
export type MakerKey = "samsung" | "xiaomi" | "huawei" | "oneplus" | "oppo" | "vivo" | "motorola" | "google" | "sony" | "other";

export interface Maker {
  key: MakerKey;
  name: string;
  /** Where "Build number" usually is. */
  buildNumber: string;
  /** Extra things this maker is known to need or block. */
  notes: string[];
}

export const MAKERS: Maker[] = [
  { key: "samsung", name: "Samsung", buildNumber: "Settings > About phone > Software information > Build number",
    notes: ["On One UI 6.1 and newer, turn off Auto Blocker (Settings > Security and privacy > Auto Blocker), or switch off its option that blocks commands from a USB cable. Otherwise the phone ignores this app.",
      "Some Samsung system apps are protected and refuse to be disabled; that is the phone, not a bug."] },
  { key: "xiaomi", name: "Xiaomi / Redmi / POCO", buildNumber: "Settings > About phone > tap the MIUI or HyperOS version",
    notes: ["In Developer options also turn on \"USB debugging (Security settings)\" (it may ask you to sign in to a Mi account). Without it, removing or disabling apps fails with a permission error.",
      "Also turn on \"Install via USB\" if you see it, and set the USB mode to File transfer."] },
  { key: "huawei", name: "Huawei / Honor", buildNumber: "Settings > About phone > Build number",
    notes: ["In Developer options turn on \"Allow ADB debugging in charge only mode\" if it is there.",
      "If a HiSuite pop-up appears on the phone, you can dismiss it. Newer Huawei phones running HarmonyOS may block parts of this."] },
  { key: "oneplus", name: "OnePlus", buildNumber: "Settings > About device > Version > Build number",
    notes: ["Set the USB mode to File transfer when you plug in.", "If you see \"Disable permission monitoring\" in Developer options, turn it on."] },
  { key: "oppo", name: "Oppo / Realme", buildNumber: "Settings > About device > Version > Build number",
    notes: ["Set the USB mode to File transfer when you plug in.", "If you see \"Disable permission monitoring\" or a second USB debugging switch (Security settings), turn it on."] },
  { key: "vivo", name: "vivo / iQOO", buildNumber: "Settings > About phone > Software version > Build number (tap it 7 times)",
    notes: ["Turn on every extra USB debugging option you see in Developer options, and set the USB mode to File transfer."] },
  { key: "motorola", name: "Motorola", buildNumber: "Settings > About phone > Build number",
    notes: ["When you plug in, pull down the notification and choose File transfer as the USB mode."] },
  { key: "google", name: "Google Pixel", buildNumber: "Settings > About phone > Build number",
    notes: ["Nothing extra is needed. Android 11 or newer also offers Wireless debugging in Developer options."] },
  { key: "sony", name: "Sony", buildNumber: "Settings > About phone > Build number", notes: ["Set the USB mode to File transfer when you plug in."] },
  { key: "other", name: "Another maker", buildNumber: "Settings > About phone > Build number (sometimes under Software information)",
    notes: ["If you cannot find Build number, search for it in Settings. Turn on every extra USB debugging option you see in Developer options."] },
];

/** Which maker's notes to show for the manufacturer name a phone reports (any case, any spelling of the brand). */
export function makerFor(manufacturer: string | null | undefined): MakerKey {
  const m = (manufacturer ?? "").trim().toLowerCase();
  if (!m) return "other";
  const has = (...words: string[]) => words.some((w) => m.includes(w));
  if (has("samsung")) return "samsung";
  if (has("xiaomi", "redmi", "poco", "blackshark")) return "xiaomi";
  if (has("huawei", "honor", "hihonor")) return "huawei";
  if (has("oneplus")) return "oneplus";
  if (has("oppo", "realme")) return "oppo";
  if (has("vivo", "iqoo")) return "vivo";
  if (has("motorola", "lenovo")) return "motorola";
  if (has("google")) return "google";
  if (has("sony")) return "sony";
  return "other";
}
