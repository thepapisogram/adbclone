import { useCallback, useEffect, useMemo, useState } from "react";
import { ipc } from "../../lib/ipc";
import { AppIcon } from "../../components/AppIcon";
import { useSettings } from "../../lib/settings";
import type { AppInfo, StepResult, WakeReport } from "../../lib/types";

const errText = (e: unknown) => String(e instanceof Error ? e.message : e);
const dur = (s?: number | null) => (s == null ? "" : s >= 3600 ? `${Math.floor(s / 3600)} h ${Math.floor((s % 3600) / 60)} min` : s >= 60 ? `${Math.floor(s / 60)} min` : `${s} s`);

interface Props { serial: string; apps: AppInfo[]; active: boolean }

export function Wake({ serial, apps, active }: Props) {
  const { plain } = useSettings();
  const [rep, setRep] = useState<WakeReport | null>(null);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const [msg, setMsg] = useState<StepResult[] | null>(null);
  const byName = useMemo(() => new Map(apps.map((a) => [a.name, a])), [apps]);

  const load = useCallback(() => {
    if (!serial) return;
    setRep(null); setError("");
    ipc.wake(serial).then(setRep).catch((e) => { setError(errText(e)); setRep({ available: false, locks: [], wakeups: [] }); });
  }, [serial]);
  useEffect(() => { if (active) load(); }, [active, load]);

  const restrict = async (pkg: string) => {
    setBusy(true); setMsg(null);
    try { setMsg(await ipc.tuning.restrict(serial, [pkg], true)); } catch (e) { setError(errText(e)); } finally { setBusy(false); }
  };

  // Locks credited to an app, longest first; system-held ones (no app) are listed apart and never actionable.
  const appLocks = (rep?.locks ?? []).filter((l) => l.package).sort((a, b) => (b.heldSecs ?? 0) - (a.heldSecs ?? 0));
  const sysLocks = (rep?.locks ?? []).filter((l) => !l.package);
  const nameOf = (p: string) => byName.get(p)?.label ?? p;
  const canRestrict = (p: string) => { const a = byName.get(p); return !!a && a.risk !== "unsafe" && a.disableSafety !== "unsafe"; };
  const Name = ({ p }: { p: string }) => <span className="namecell"><AppIcon name={p} />{nameOf(p)}{!plain && <span className="mono muted"> {p}</span>}</span>;
  const Act = ({ p }: { p: string }) => canRestrict(p)
    ? <button disabled={busy} onClick={() => restrict(p)}>Stop it running in the background</button>
    : <span className="muted small">core app: left alone</span>;

  if (!serial) return <div className="empty-state">Select a device to see what keeps it awake.</div>;
  return (
    <div className="storage">
      {error && <div className="error" role="alert">{error}</div>}
      <div className="row"><button disabled={!rep} onClick={load}>Refresh</button><span className="muted small">A snapshot of this moment. Some apps hold the phone awake for good reasons (music, navigation, downloads).</span></div>
      {msg && <ul className="steps-list" aria-label="Results">{msg.map((r, i) => <li key={i} className={r.ok ? "ok" : "bad"}>{r.ok ? "✓" : "✗"} {r.label}{r.message && <span className="muted"> — {r.message}</span>}</li>)}</ul>}
      {rep === null ? <p className="muted">Asking the phone…</p> : !rep.available ? (
        <p className="muted">This phone did not report this information (some makers keep it private). Nothing is guessed.</p>
      ) : (
        <>
          <section className="block">
            <h3>Keeping the phone awake right now</h3>
            {appLocks.length === 0 ? <p className="status ok">No app is holding the phone awake at this moment.</p> : (
              <table className="storagetable" aria-label="Held wake locks">
                <tbody>{appLocks.map((l, i) => (
                  <tr key={i} className="static"><td><Name p={l.package!} /></td><td className="muted">{l.kind === "PARTIAL_WAKE_LOCK" ? "keeping the processor on" : "keeping the screen on"}{l.heldSecs != null && ` for ${dur(l.heldSecs)}`}</td><td className="muted mono">{plain ? "" : l.tag}</td><td><Act p={l.package!} /></td></tr>
                ))}</tbody>
              </table>
            )}
            {sysLocks.length > 0 && <p className="muted small">{sysLocks.length} more held by Android itself (radio, display, sensors). These are normal and cannot be changed here.</p>}
          </section>
          <section className="block">
            <h3>Apps that wake the phone up most</h3>
            {rep.wakeups.length === 0 ? <p className="muted">No wake-up counts were reported.</p> : (
              <table className="storagetable" aria-label="Wake-ups">
                <thead><tr><th>App</th><th className="num">Wake-ups</th><th className="num">Alarms set</th><th /></tr></thead>
                <tbody>{rep.wakeups.filter((w) => byName.has(w.package)).slice(0, 15).map((w) => (
                  <tr key={w.package} className="static"><td><Name p={w.package} /></td><td className="num"><b>{w.wakeups}</b></td><td className="num">{w.alarms}</td><td><Act p={w.package} /></td></tr>
                ))}</tbody>
              </table>
            )}
            <p className="muted small">Counts since the phone last reset its statistics. Each wake-up uses some battery and makes a little heat. Blocking background activity is reversible from the Tune tab.</p>
          </section>
        </>
      )}
    </div>
  );
}
