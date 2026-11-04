/** Plain-language meaning of the permissions people care about. Everything else is counted, not listed. */
export interface Perm { name: string; granted: boolean }
interface Meaning { label: string; sensitive: boolean }

const M: Record<string, Meaning> = {
  CAMERA: { label: "Use the camera", sensitive: true },
  RECORD_AUDIO: { label: "Record audio", sensitive: true },
  ACCESS_FINE_LOCATION: { label: "Know your exact location", sensitive: true },
  ACCESS_COARSE_LOCATION: { label: "Know your approximate location", sensitive: true },
  ACCESS_BACKGROUND_LOCATION: { label: "Know your location even when closed", sensitive: true },
  READ_CONTACTS: { label: "Read your contacts", sensitive: true },
  WRITE_CONTACTS: { label: "Change your contacts", sensitive: true },
  GET_ACCOUNTS: { label: "See the accounts on the phone", sensitive: true },
  READ_SMS: { label: "Read your text messages", sensitive: true },
  SEND_SMS: { label: "Send text messages", sensitive: true },
  RECEIVE_SMS: { label: "Receive text messages", sensitive: true },
  READ_CALL_LOG: { label: "Read your call history", sensitive: true },
  WRITE_CALL_LOG: { label: "Change your call history", sensitive: true },
  CALL_PHONE: { label: "Make phone calls", sensitive: true },
  READ_PHONE_STATE: { label: "See phone status and number", sensitive: true },
  ANSWER_PHONE_CALLS: { label: "Answer calls", sensitive: true },
  READ_CALENDAR: { label: "Read your calendar", sensitive: true },
  WRITE_CALENDAR: { label: "Change your calendar", sensitive: true },
  BODY_SENSORS: { label: "Use body sensors", sensitive: true },
  ACTIVITY_RECOGNITION: { label: "Know when you walk or drive", sensitive: true },
  READ_EXTERNAL_STORAGE: { label: "Read your files and media", sensitive: true },
  WRITE_EXTERNAL_STORAGE: { label: "Change your files and media", sensitive: true },
  MANAGE_EXTERNAL_STORAGE: { label: "Manage all your files", sensitive: true },
  READ_MEDIA_IMAGES: { label: "Read your photos", sensitive: true },
  READ_MEDIA_VIDEO: { label: "Read your videos", sensitive: true },
  READ_MEDIA_AUDIO: { label: "Read your music and audio", sensitive: true },
  BLUETOOTH_CONNECT: { label: "Connect to Bluetooth devices", sensitive: false },
  BLUETOOTH_SCAN: { label: "Find nearby Bluetooth devices", sensitive: false },
  NEARBY_WIFI_DEVICES: { label: "Find nearby Wi-Fi devices", sensitive: false },
  SYSTEM_ALERT_WINDOW: { label: "Draw over other apps", sensitive: true },
  REQUEST_INSTALL_PACKAGES: { label: "Install other apps", sensitive: true },
  QUERY_ALL_PACKAGES: { label: "See all apps you have installed", sensitive: false },
  BIND_ACCESSIBILITY_SERVICE: { label: "Read and control the screen (accessibility)", sensitive: true },
  POST_NOTIFICATIONS: { label: "Show notifications", sensitive: false },
  RECEIVE_BOOT_COMPLETED: { label: "Start by itself when the phone turns on", sensitive: false },
  WAKE_LOCK: { label: "Keep the phone awake", sensitive: false },
  FOREGROUND_SERVICE: { label: "Run in the background with a notification", sensitive: false },
  USE_BIOMETRIC: { label: "Use fingerprint or face unlock", sensitive: false },
  INTERNET: { label: "Use the internet", sensitive: false },
};

const short = (n: string) => (n.startsWith("android.permission.") ? n.slice("android.permission.".length) : "");

export interface Described { label: string; granted: boolean; sensitive: boolean }
export interface PermissionSummary { known: Described[]; otherCount: number }

export function describePermissions(perms: Perm[]): PermissionSummary {
  const known: Described[] = [];
  let otherCount = 0;
  const seen = new Set<string>();
  for (const p of perms) {
    const m = M[short(p.name)];
    if (!m) { otherCount++; continue; }
    if (seen.has(m.label)) continue;
    seen.add(m.label);
    known.push({ label: m.label, granted: p.granted, sensitive: m.sensitive });
  }
  known.sort((a, b) => Number(b.sensitive) - Number(a.sensitive) || Number(b.granted) - Number(a.granted) || a.label.localeCompare(b.label));
  return { known, otherCount };
}

/** One generated sentence about what an app can do, from what the phone actually says. Empty if nothing notable. */
export function whatItCanDo(perms: Perm[]): string {
  const d = describePermissions(perms).known.filter((k) => k.sensitive && k.granted).map((k) => k.label.toLowerCase());
  if (!d.length) return "";
  const shown = d.slice(0, 4).join(", ");
  return `Allowed right now to: ${shown}${d.length > 4 ? ` and ${d.length - 4} more` : ""}.`;
}
