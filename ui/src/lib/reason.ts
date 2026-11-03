import type { AppInfo } from "./types";

/** The one-line explanation shown under an app's name in plain-language mode. */
export function reasonOf(a: Pick<AppInfo, "description" | "risk">): string {
  const d = a.description?.trim();
  if (d) return d;
  return a.risk === "unknown" ? "No information about this app yet." : "";
}
