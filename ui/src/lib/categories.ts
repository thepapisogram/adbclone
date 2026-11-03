/** Plain-words names for rating categories. */
export const categoryName: Record<string, string> = {
  "ads-telemetry": "ads and reports", "partner-stub": "partner helper", "help-promo": "tips and promos", discontinued: "discontinued", "aosp-extra": "leftover extra",
  "game-hub": "games", assistant: "assistant", "vendor-app": "maker's app", "google-app": "Google app", "preinstalled-app": "preinstalled app",
  store: "app store", carrier: "carrier", "cloud-sync": "maker's cloud", payments: "payments", "system-service": "system service", "core-app": "main app",
  "background-service": "background helper", "core-system": "core system", duplicate: "duplicate", other: "other",
};
export const nameOfCategory = (c: string) => categoryName[c] ?? c;
