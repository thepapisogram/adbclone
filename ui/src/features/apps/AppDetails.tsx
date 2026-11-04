import { useEffect, useState } from "react";
import { ipc } from "../../lib/ipc";
import { AppIcon } from "../../components/AppIcon";
import { installerKey } from "../../lib/filters";
import { nameOfCategory } from "../../lib/categories";
import { describePermissions, whatItCanDo } from "./permissions";
import type { AppDetailsOut, AppInfo } from "../../lib/types";

const errText = (e: unknown) => String(e instanceof Error ? e.message : e);
const date = (ms?: number) => (ms && ms > 0 ? new Date(ms).toLocaleDateString(undefined, { year: "numeric", month: "short", day: "numeric" }) : "—");
const INSTALLED_BY: Record<string, string> = { play: "Google Play", "maker-store": "The phone maker's store", file: "An app file (side-loaded)", "other-store": "Another store", preinstalled: "Came with the phone" };
const TIER: Record<string, string> = { safe: "Safe to remove", balanced: "Balanced", aggressive: "Aggressive", never: "Never remove", unrated: "Not rated" };

interface Props { serial: string; app: AppInfo; onClose: () => void }

/** Everything the phone says about one app, in plain words. Opens beside the list. */
export function AppDetails({ serial, app, onClose }: Props) {
  const [info, setInfo] = useState<AppDetailsOut | null>(null);
  const [error, setError] = useState("");
  const [copied, setCopied] = useState(false);
  useEffect(() => {
    setInfo(null); setError("");
    let cancelled = false;
    ipc.appDetails(serial, app.name).then((d) => !cancelled && setInfo(d)).catch((e) => !cancelled && setError(errText(e)));
    return () => { cancelled = true; };
  }, [serial, app.name]);
  useEffect(() => {
    const k = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", k);
    return () => window.removeEventListener("keydown", k);
  }, [onClose]);

  const d = app.details;
  const ik = installerKey(app);
  const perms = info ? describePermissions(info.permissions) : null;
  const generated = info ? whatItCanDo(info.permissions) : "";
  const about = info?.description || app.description || "";
  const link = `https://play.google.com/store/apps/details?id=${app.name}`;

  return (
    <aside className="drawer" role="dialog" aria-label={`Details of ${app.label}`}>
      <header className="drawer-head">
        <AppIcon name={app.name} size={48} />
        <div className="drawer-title"><h2>{app.label}</h2><div className="mono muted">{app.name}</div></div>
        <button onClick={onClose} aria-label="Close details">✕</button>
      </header>
      <div className="drawer-body">
        <p>
          <span className={`pill ${app.state}`}>{app.state}</span>{" "}
          <span className="pill">{app.system ? "came with the phone" : "installed by you"}</span>{" "}
          {d && !d.launchable && <span className="pill">no launcher icon</span>}
        </p>

        <h3>About</h3>
        {about ? <p>{about}</p> : <p className="muted">The phone does not store a description for this app.</p>}
        {!info?.description && !app.description && generated && <p className="muted small">{generated} <i>(worked out from its permissions)</i></p>}

        <h3>On this phone</h3>
        <dl className="facts">
          <dt>Version</dt><dd>{d?.versionName || "—"}{d?.versionCode ? ` (${d.versionCode})` : ""}</dd>
          <dt>Installed by</dt><dd>{ik ? INSTALLED_BY[ik] : "—"}{d?.installer ? <span className="muted"> · {d.installer}</span> : null}</dd>
          <dt>First installed</dt><dd>{date(d?.firstInstall)}</dd>
          <dt>Last updated</dt><dd>{date(d?.lastUpdate)}</dd>
          <dt>Built for Android API</dt><dd>{d?.targetSdk || "—"}</dd>
        </dl>
        {!d && <p className="muted small">Version and install details appear once the phone has reported them.</p>}

        <h3>If you remove it</h3>
        <dl className="facts">
          <dt>Rating</dt><dd><span className={`pill risk-${app.risk}`}>{app.risk === "unknown" ? "unrated" : app.risk}</span> {TIER[app.tier] ?? app.tier}</dd>
          {app.category && <><dt>Kind of app</dt><dd>{nameOfCategory(app.category)}</dd></>}
          <dt>What stops working</dt><dd>{app.impact || "Nobody has rated this app yet."}</dd>
          {app.reinstall && <><dt>Getting it back</dt><dd>{app.reinstall === "store" ? "Reinstall it from an app store, or restore a snapshot." : "Only a snapshot restore brings it back."}</dd></>}
        </dl>

        <h3>Permissions</h3>
        {error && <p className="status error" role="alert">{error}</p>}
        {!info && !error && <p className="muted">Asking the phone…</p>}
        {perms && (perms.known.length === 0 && perms.otherCount === 0
          ? <p className="muted">This app asks for no permissions.</p>
          : <>
              <ul className="perms" aria-label="Permissions">
                {perms.known.map((k) => <li key={k.label} className={k.granted ? "yes" : "no"}><span aria-hidden="true">{k.granted ? "●" : "○"}</span> {k.label}<span className="muted small"> {k.granted ? "allowed" : "not allowed"}</span></li>)}
              </ul>
              {perms.otherCount > 0 && <p className="muted small">…and {perms.otherCount} other technical permission{perms.otherCount === 1 ? "" : "s"}.</p>}
              <p className="muted small">Read from the phone via {info!.from === "helper" ? "the temporary helper" : "Android's package report"}.</p>
            </>)}

        <div className="row" style={{ marginTop: 12 }}>
          <button onClick={() => { navigator.clipboard?.writeText(link).then(() => setCopied(true)).catch(() => {}); }}>{copied ? "Link copied" : "Copy Google Play link"}</button>
        </div>
      </div>
    </aside>
  );
}
