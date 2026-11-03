import { useEffect, useRef, useState } from "react";
import { ipc } from "../../lib/ipc";
import type { AdbDownloadEvent, AdbStatus } from "../../lib/types";

const mb = (n: number) => `${(n / 1_048_576).toFixed(1)} MB`;
const where = { env: "the ADB_PATH setting", appData: "downloaded by this app", nextToApp: "next to this app", path: "installed on your computer" } as const;

interface Props {
  status: AdbStatus | null;
  /** Called when adb has just been set up, so the caller can refresh. */
  onInstalled: () => void;
}

/** Shows whether adb (the tool that talks to the phone) is available, and sets it up automatically if not. */
export function AdbSetup({ status, onInstalled }: Props) {
  const [ev, setEv] = useState<AdbDownloadEvent | null>(null);
  const [starting, setStarting] = useState(false);
  const [error, setError] = useState("");
  const off = useRef<() => void>(() => {});
  useEffect(() => () => off.current(), []);

  const start = async () => {
    setError(""); setStarting(true); setEv({ stage: "downloading", received: 0 });
    try {
      off.current = await ipc.adb.download((e) => {
        setEv(e);
        if (e.stage === "error") { setError(e.message ?? "The download failed."); setStarting(false); }
        if (e.stage === "done") { setStarting(false); onInstalled(); }
      });
    } catch (e) { setError(String(e instanceof Error ? e.message : e)); setStarting(false); setEv(null); }
  };

  if (status?.found) {
    return (
      <p className="status ok" role="status">
        adb is ready ({status.version ?? "version unknown"}), {status.source ? where[status.source] : "found"}.
      </p>
    );
  }
  const pct = ev?.total ? Math.min(100, Math.round((ev.received / ev.total) * 100)) : null;
  return (
    <div className="adbsetup">
      <p>
        Coolant talks to your phone through <b>adb</b>, a small tool from Google. It was not found on this computer.
        The app can download Google's official copy for you (about 8 MB) and keep it in its own folder. Nothing else on your computer is changed.
      </p>
      {ev?.stage === "downloading" && (
        <div role="progressbar" aria-valuenow={pct ?? undefined} aria-label="Download progress">
          <div className="track"><div className="fill" style={{ width: `${pct ?? 5}%` }} /></div>
          <span className="muted small">{ev.total ? `${mb(ev.received)} of ${mb(ev.total)}` : `${mb(ev.received)}…`}</span>
        </div>
      )}
      {error && <div className="status error" role="alert">{error} You can also install the Android "platform-tools" yourself and make sure <code>adb</code> is on your PATH.</div>}
      <div className="row">
        <button className="primary" onClick={start} disabled={starting}>{starting ? "Downloading…" : error ? "Try again" : "Download adb from Google"}</button>
      </div>
      <p className="muted small">Comes from dl.google.com over a secure connection and is checked by running it. By downloading you accept the Android SDK licence (developer.android.com/studio/releases/platform-tools).</p>
    </div>
  );
}
