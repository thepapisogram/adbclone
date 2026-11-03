import { useCallback, useEffect, useMemo, useState } from "react";
import { ipc } from "../../lib/ipc";
import type { AppInfo, HealthSample, RunningRow, StepResult, TuningInfo } from "../../lib/types";

const errText = (e: unknown) => String(e instanceof Error ? e.message : e);
const mb = (kb: number) => (kb >= 1_048_576 ? `${(kb / 1_048_576).toFixed(2)} GB` : `${Math.round(kb / 1024)} MB`);

interface Reading { hot: number | null; batt: number | null; availKb: number | null; at: number }

function readingFrom(h: HealthSample | null, availKb: number | null): Reading {
  return { hot: h?.hottest?.tempC ?? null, batt: h?.battery?.tempC ?? null, availKb, at: Date.now() };
}
const deg = (v: number | null) => (v == null ? "—" : `${v.toFixed(1)}°C`);
const delta = (a: number | null, b: number | null) => (a == null || b == null ? "" : ` (${b - a >= 0 ? "+" : ""}${(b - a).toFixed(1)})`);

interface Props { serial: string; apps: AppInfo[]; active: boolean }

export function Tune({ serial, apps, active }: Props) {
  const [info, setInfo] = useState<TuningInfo | null>(null);
  const [rows, setRows] = useState<RunningRow[]>([]);
  const [availKb, setAvailKb] = useState<number | null>(null);
  const [picked, setPicked] = useState<Set<string>>(new Set());
  const [touched, setTouched] = useState(false);
  const [stopToo, setStopToo] = useState(false);
  const [busy, setBusy] = useState(false);
  const [confirm, setConfirm] = useState(false);
  const [results, setResults] = useState<StepResult[] | null>(null);
  const [error, setError] = useState("");
  const [before, setBefore] = useState<Reading | null>(null);
  const [after, setAfter] = useState<Reading | null>(null);
  const [cleanMsg, setCleanMsg] = useState("");

  const byName = useMemo(() => new Map(apps.map((a) => [a.name, a])), [apps]);
  const reload = useCallback(() => ipc.tuning.status(serial).then(setInfo).catch((e) => setError(errText(e))), [serial]);
  useEffect(() => { setInfo(null); setResults(null); setBefore(null); setAfter(null); setTouched(false); setPicked(new Set()); if (serial) reload(); }, [serial, reload]);

  // Sample what is running (CPU needs two samples, so keep polling while this tab is open).
  useEffect(() => {
    if (!active || !serial) return;
    let stopped = false;
    let timer: ReturnType<typeof setTimeout>;
    const loop = async () => {
      try { const r = await ipc.listRunning(serial); if (!stopped) { setRows(r.rows); setAvailKb(r.memAvailableKb); } } catch { /* shown elsewhere */ }
      if (!stopped) timer = setTimeout(loop, 4000);
    };
    loop();
    return () => { stopped = true; clearTimeout(timer); };
  }, [active, serial]);

  const restrictedNow = useMemo(() => new Set((info?.records ?? []).filter((r) => r.group === "background" && "pkg" in r.target).map((r) => (r.target as { pkg: string }).pkg)), [info]);
  const protectedApp = (n: string) => { const a = byName.get(n); return !!a && (a.risk === "unsafe" || a.disableSafety === "unsafe"); };

  const offenders = useMemo(() => rows
    .filter((r) => r.package && !protectedApp(r.package))
    .sort((a, b) => (b.cpu ?? -1) - (a.cpu ?? -1) || b.rssKb - a.rssKb)
    .slice(0, 15), [rows, byName]); // eslint-disable-line react-hooks/exhaustive-deps

  // Pre-tick the busiest apps the user installed, until they make their own choice.
  useEffect(() => {
    if (touched) return;
    const top = offenders.filter((r) => !byName.get(r.package!)?.system && (r.cpu ?? 0) >= 1).slice(0, 5).map((r) => r.package!);
    setPicked(new Set(top));
  }, [offenders, touched, byName]);

  const run = async (fn: () => Promise<StepResult[]>) => {
    setBusy(true); setError("");
    try { setResults(await fn()); await reload(); } catch (e) { setError(errText(e)); } finally { setBusy(false); }
  };
  const toggle = (id: string, on: boolean) => run(() => ipc.tuning.toggle(serial, id, on));

  const startCooldown = async () => {
    setConfirm(false); setAfter(null);
    const h = await ipc.healthSample(serial).catch(() => null);
    setBefore(readingFrom(h, availKb));
    await run(() => ipc.tuning.cooldown(serial, [...picked], stopToo));
  };
  const checkAgain = async () => {
    const h = await ipc.healthSample(serial).catch(() => null);
    const r = await ipc.listRunning(serial).catch(() => null);
    setAfter(readingFrom(h, r?.memAvailableKb ?? availKb));
  };

  if (!serial) return <div className="empty-state">Select a device to tune it.</div>;
  const has = (g: string) => !!info?.records.some((r) => r.group === g);
  const records = info?.records ?? [];
  const toggles = [
    { id: "animations", title: "Calmer animations (0.5x)", text: "Halves window and transition animation time. The phone feels quicker and the GPU does less work.", disabled: false },
    { id: "battery_saver", title: "Battery saver", text: "Limits background work and lowers peak performance, which reduces heat. Some makers ignore this from a computer; you will be told.", disabled: false },
    { id: "refresh", title: "Cap screen at 60 Hz", text: info && !info.refreshSupported ? "Your screen has no refresh rate above 60 Hz, so there is nothing to cap." : "A 90 or 120 Hz screen uses more power and makes more heat.", disabled: !!info && !info.refreshSupported },
  ];

  return (
    <div className="tune">
      {error && <div className="error" role="alert">{error}</div>}
      {records.length > 0 && (
        <div className="notice">
          {records.length} change{records.length > 1 ? "s" : ""} from this app {records.length > 1 ? "are" : "is"} active on the phone.{" "}
          <button className="linklike" disabled={busy} onClick={() => run(() => ipc.tuning.undo(serial, {}))}>Undo all changes</button>
        </div>
      )}

      <section className="block">
        <h3>Quick settings</h3>
        {toggles.map((t) => (
          <label key={t.id} className={`switchrow ${t.disabled ? "off" : ""}`}>
            <input type="checkbox" role="switch" checked={has(t.id)} disabled={busy || t.disabled || !info} onChange={(e) => toggle(t.id, e.target.checked)} aria-label={t.title} />
            <span><b>{t.title}</b><br /><span className="muted small">{t.text}</span></span>
          </label>
        ))}
      </section>

      <section className="block">
        <h3>Cool-down mode</h3>
        <p className="muted small">
          Turns on the three settings above and stops the busiest apps from running in the background. Nothing is uninstalled, and everything can be undone.
          Cooling takes a few minutes: stop using the phone heavily, take it off the charger if you can, and check again after a short wait.
        </p>
        <table>
          <thead><tr><th className="chk" /><th>App</th><th className="num">CPU %</th><th className="num">RAM</th><th>Background</th></tr></thead>
          <tbody>
            {offenders.map((r) => {
              const n = r.package!; const a = byName.get(n);
              return (
                <tr key={n} className={picked.has(n) ? "sel" : ""} onClick={() => { setTouched(true); setPicked((s) => { const x = new Set(s); x.has(n) ? x.delete(n) : x.add(n); return x; }); }}>
                  <td className="chk"><input type="checkbox" checked={picked.has(n)} aria-label={`Select ${a?.label ?? n}`} onChange={() => {}} /></td>
                  <td><div>{a?.label ?? n} {a?.system && <span className="pill">system</span>}</div><div className="mono muted">{n}</div></td>
                  <td className="num">{r.cpu == null ? "—" : r.cpu.toFixed(1)}</td>
                  <td className="num">{mb(r.rssKb)}</td>
                  <td>{restrictedNow.has(n) ? <span className="pill thermal-ok">restricted</span> : <span className="muted">normal</span>}</td>
                </tr>
              );
            })}
            {!offenders.length && <tr><td colSpan={5} className="empty">Looking at what is running…</td></tr>}
          </tbody>
        </table>
        <div className="row" style={{ marginTop: 10 }}>
          <label className="inline"><input type="checkbox" checked={stopToo} onChange={(e) => setStopToo(e.target.checked)} /> Also stop the ticked apps now</label>
          <div className="spacer" />
          <button className="primary" disabled={busy || !info} onClick={() => setConfirm(true)}>Start cool-down ({picked.size} app{picked.size === 1 ? "" : "s"})</button>
        </div>
        {(before || results) && (
          <div className="beforeafter">
            {before && <p>Before: hottest sensor <b>{deg(before.hot)}</b>, battery <b>{deg(before.batt)}</b>{before.availKb != null && <>, free RAM <b>{mb(before.availKb)}</b></>}</p>}
            {after && before && <p>After {Math.max(1, Math.round((after.at - before.at) / 60000))} min: hottest <b>{deg(after.hot)}{delta(before.hot, after.hot)}</b>, battery <b>{deg(after.batt)}{delta(before.batt, after.batt)}</b>{after.availKb != null && <>, free RAM <b>{mb(after.availKb)}</b></>}</p>}
            {before && <button disabled={busy} onClick={checkAgain}>Check temperature again</button>}
          </div>
        )}
        {results && (
          <ul className="steps-list" aria-label="Results">
            {results.map((r, i) => <li key={i} className={r.ok ? "ok" : "bad"}>{r.ok ? "✓" : "✗"} {r.label}{r.message && <span className="muted"> — {r.message}</span>}</li>)}
          </ul>
        )}
      </section>

      {restrictedNow.size > 0 && (
        <section className="block">
          <h3>Apps with restricted background activity</h3>
          {[...restrictedNow].map((n) => (
            <div key={n} className="saved">
              <div><b>{byName.get(n)?.label ?? n}</b><div className="mono muted">{n}</div></div>
              <button disabled={busy} onClick={() => run(() => ipc.tuning.restrict(serial, [n], false))}>Allow again</button>
            </div>
          ))}
          <p className="muted small">Restricted apps may show notifications late or sync only when opened. Android can also loosen the restriction by itself over time.</p>
        </section>
      )}

      <section className="block">
        <h3>One-off cleanup</h3>
        <div className="row">
          <button disabled={busy} onClick={async () => { setBusy(true); setCleanMsg("Clearing caches…"); try { await ipc.tuning.trimCaches(serial); setCleanMsg("Caches cleared. Apps may open a little slower the first time."); } catch (e) { setCleanMsg(errText(e)); } finally { setBusy(false); } }}>Clear all app caches</button>
          <span className="muted small">{cleanMsg || "Frees storage. Cannot be undone, but caches rebuild by themselves."}</span>
        </div>
      </section>

      {confirm && (
        <div className="modal-bg" onClick={() => setConfirm(false)}>
          <div className="modal" role="dialog" aria-label="Confirm cool-down" onClick={(e) => e.stopPropagation()}>
            <h2>Start cool-down?</h2>
            <ul>
              <li>Battery saver on</li><li>Animations at half time</li>
              {info?.refreshSupported && <li>Screen capped at 60 Hz</li>}
              <li>Background activity blocked for {picked.size} app{picked.size === 1 ? "" : "s"}{stopToo && ", and they are stopped now"}</li>
            </ul>
            <p className="muted">Reversible: use "Undo all changes" at any time. Core system apps are never restricted.</p>
            <div className="modal-actions"><button onClick={() => setConfirm(false)}>Cancel</button><button className="primary" onClick={startCooldown}>Start</button></div>
          </div>
        </div>
      )}
    </div>
  );
}
