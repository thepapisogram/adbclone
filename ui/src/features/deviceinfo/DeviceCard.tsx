import { useEffect, useState } from "react";
import { ipc } from "../../lib/ipc";
import type { DeviceInfo } from "../../lib/types";
import { ReportDialog } from "./ReportDialog";

const gb = (kb: number) => `${(kb / 1_048_576).toFixed(kb >= 10_485_760 ? 0 : 1)} GB`;

/** One-line summary of the selected phone, with a warning for very old Android versions. */
export function DeviceCard({ serial }: { serial: string }) {
  const [info, setInfo] = useState<DeviceInfo | null>(null);
  const [reporting, setReporting] = useState(false);

  useEffect(() => {
    setInfo(null);
    if (!serial) return;
    let cancelled = false;
    ipc.deviceInfo(serial).then((i) => !cancelled && setInfo(i)).catch(() => {});
    return () => { cancelled = true; };
  }, [serial]);

  if (!serial) return null;
  const used = info?.storageUsedKb, total = info?.storageTotalKb;
  const parts = [
    [info?.manufacturer, info?.model].filter(Boolean).join(" "),
    info?.androidRelease ? `Android ${info.androidRelease}${info.sdk ? ` (API ${info.sdk})` : ""}` : null,
    info?.securityPatch ? `patch ${info.securityPatch}` : null,
    used != null && total ? `storage ${gb(used)} of ${gb(total)} used (${Math.round((used / total) * 100)}%)` : null,
    info?.batteryLevel != null ? `battery ${info.batteryLevel}%` : null,
  ].filter(Boolean);
  return (
    <div className="devicecard">
      <span>{parts.length ? parts.join(" · ") : "Device"}</span>
      {info?.sdk != null && info.sdk < 24 && <span className="warn-inline"> Android 7 or newer is recommended; some features may not work here.</span>}
      <span className="spacer" />
      <button className="small" onClick={() => setReporting(true)} title="Collect what this app reads from the phone, to share when something looks wrong">Save device report</button>
      {reporting && <ReportDialog serial={serial} onClose={() => setReporting(false)} />}
    </div>
  );
}
