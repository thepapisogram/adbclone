import type { Action, ActionResult, AppInfo, Device } from "./types";

const apps: AppInfo[] = [
  ["com.google.android.youtube", "YouTube", true, "recommended", "Video app; reinstallable from Play Store."],
  ["com.google.android.apps.maps", "Maps", true, "recommended", "Google Maps."],
  ["com.google.android.gm", "Gmail", true, "advanced", "Email client."],
  ["com.google.android.gms", "Google Play services", true, "unsafe", "Core services; many apps break without it."],
  ["com.google.android.gsf", "Google Services Framework", true, "unsafe", "Required for Google account sync and push."],
  ["com.android.vending", "Google Play Store", true, "advanced", "App store."],
  ["com.android.systemui", "System UI", true, "unsafe", "Status bar, navigation and lock screen. Disabling bricks the UI."],
  ["com.android.settings", "Settings", true, "unsafe", "Device settings."],
  ["com.android.phone", "Phone Services", true, "unsafe", "Telephony stack."],
  ["com.android.chrome", "Chrome", true, "advanced", "Default browser."],
  ["com.facebook.appmanager", "Facebook App Manager", true, "recommended", "Preinstalled background updater."],
  ["com.facebook.services", "Facebook Services", true, "recommended", "Preinstalled background service."],
  ["com.facebook.system", "Facebook Installer", true, "recommended", "Preinstalled installer stub."],
  ["com.samsung.android.bixby.agent", "Bixby Voice", true, "recommended", "Voice assistant."],
  ["com.samsung.android.game.gamehome", "Game Launcher", true, "recommended", "Game launcher overlay."],
  ["com.microsoft.office.officehubrow", "Microsoft 365", true, "recommended", "Preinstalled Office hub."],
  ["com.netflix.partner.activation", "Netflix Activation", true, "recommended", "Partner activation stub."],
  ["com.google.android.apps.photos", "Photos", true, "advanced", "Gallery and backup."],
  ["com.android.stk", "SIM Toolkit", true, "advanced", "Carrier SIM menu."],
  ["org.mozilla.firefox", "Firefox", false, "unknown", ""],
  ["com.spotify.music", "Spotify", false, "unknown", ""],
  ["org.telegram.messenger", "Telegram", false, "unknown", ""],
  ["com.whatsapp", "WhatsApp", false, "unknown", ""],
  ["com.discord", "Discord", false, "unknown", ""],
  ["com.instagram.android", "Instagram", false, "unknown", ""],
].map(([name, label, system, risk, description]) => ({
  name: name as string,
  label: label as string,
  system: system as boolean,
  state: "enabled" as const,
  risk: risk as AppInfo["risk"],
  description: description as string,
}));
apps[18].state = "disabled";

const delay = (ms = 250) => new Promise((r) => setTimeout(r, ms));

export const mock = {
  async listDevices(): Promise<Device[]> {
    await delay();
    return [
      { serial: "R58M123ABC", state: "device", model: "Pixel 7 (mock)" },
      { serial: "192.168.1.20:5555", state: "unauthorized", model: null },
    ];
  },
  async listApps(_serial: string): Promise<AppInfo[]> {
    await delay();
    return apps.map((a) => ({ ...a }));
  },
  async applyAction(_serial: string, action: Action, names: string[]): Promise<ActionResult[]> {
    await delay(400);
    return names.map((name) => {
      const a = apps.find((x) => x.name === name);
      if (!a) return { name, ok: false, message: "Package not found" };
      if (name === "com.android.systemui" && action !== "enable")
        return { name, ok: false, message: "Failure [DELETE_FAILED_DEVICE_POLICY_MANAGER]" };
      a.state = action === "disable" ? "disabled" : action === "uninstall" ? "uninstalled" : "enabled";
      return { name, ok: true };
    });
  },
};
