import { useCallback, useEffect, useMemo, useState } from "react";
import { ipc } from "../../lib/ipc";
import type { HealthSample } from "../../lib/types";

interface Point { t: number; batt: number | null; hot: number | null; hotName: string | null; status: number | null }
const MAX_POINTS = 120;

/** A small inline-SVG line chart; no chart library needed. */
function Sparkline({ values, label, unit = "°C" }: { values: (number | null)[]; label: string; unit?: string }) {
  const nums = values.filter((v): v is number => v != null);
  if (nums.length < 2) return <div className="spark empty">Collecting data…</div>;
  const lo = Math.min(...nums), hi = Math.max(...nums);
  const pad = Math.max(1, (hi - lo) * 0.15);
  const min = lo - pad, max = hi + pad;
  const W = 300, H = 70;
  const x = (i: number) => (i / (values.length - 1)) * W;
  const y = (v: number) => H - ((v - min) / (max - min)) * H;
  let d = "";
  values.forEach((v, i) => { if (v != null) d += `${d && values[i - 1] != null ? "L" : "M"}${x(i).toFixed(1)} ${y(v).toFixed(1)} `; });
  const last = nums[nums.length - 1];
  return (
    <svg className="spark" viewBox={`0 0 ${W} ${H}`} role="img" aria-label={`${label}: now ${last.toFixed(1)}${unit}, min ${lo.toFixed(1)}, max ${hi.toFixed(1)}`} preserveAspectRatio="none">
      <path d={d} fill="none" stroke="currentColor" strokeWidth="2" vectorEffect="non-scaling-stroke" />
    </svg>
  );
}

const stat = (nums: number[]) => (nums.length ? { now: nums[nums.length - 1], min: Math.min(...nums), max: Math.max(...nums) } : null);
const fmt = (v?: number | null) => (v == null ? "—" : `${v.toFixed(1)}°C`);

function StatusBadge({ s }: { s: HealthSample }) {
  if (s.thermalStatus == null) return <span className="pill">thermal status n/a</span>;
  const level = s.thermalStatus === 0 ? "ok" : s.thermalStatus === 1 ? "ok" : s.thermalStatus === 2 ? "warn" : "bad";
  const text = s.thermalStatus === 0 ? "Normal" : `Throttling: ${s.thermalStatusName}`;
  return <span className={`pill thermal-${level}`} title="Android's thermal status. Above 'light' the phone is slowing itself down to cool.">{text}</span>;
}

interface Props { serial: string; wireless?: boolean; active: boolean; onShowRunning: () => void }

export function Health({ serial, wireless, active, onShowRunning }: Props) {
  const [points, setPoints] = useState<Point[]>([]);
  const [last, setLast] = useState<HealthSample | null>(null);
  const [error, setError] = useState("");
  const [interval, setIntervalSec] = useState(wireless ? 5 : 3);
  useEffect(() => { setIntervalSec(wireless ? 5 : 3); }, [wireless]);

  const poll = useCallback(async () => {
    try {
      const s = await ipc.healthSample(serial);
      setLast(s); setError("");
      setPoints((p) => [...p, { t: Date.now(), batt: s.battery?.tempC ?? null, hot: s.hottest?.tempC ?? null, hotName: s.hottest?.name ?? null, status: s.thermalStatus }].slice(-MAX_POINTS));
    } catch (e) { setError(String(e)); }
  }, [serial]);

  // Poll only while this tab is visible and a device is selected.
  useEffect(() => {
    if (!active || !serial) return;
    let stopped = false;
    let timer: ReturnType<typeof setTimeout>;
    const loop = async () => { await poll(); if (!stopped) timer = setTimeout(loop, interval * 1000); };
    loop();
    return () => { stopped = true; clearTimeout(timer); };
  }, [active, serial, interval, poll]);
  useEffect(() => { setPoints([]); setLast(null); }, [serial]);

  const batt = useMemo(() => stat(points.map((p) => p.batt).filter((v): v is number => v != null)), [points]);
  const hot = useMemo(() => stat(points.map((p) => p.hot).filter((v): v is number => v != null)), [points]);
  const sensors = useMemo(() => [...(last?.sensors ?? [])].sort((a, b) => b.tempC - a.tempC), [last]);

  if (!serial) return <div className="empty-state">Select a device to see its temperature.</div>;

  return (
    <div className="health">
      <div className="actions">
        {last ? <StatusBadge s={last} /> : <span className="muted">Loading…</span>}
        {last?.battery && (
          <span>
            Battery {last.battery.level ?? "—"}% · {last.battery.charging ? `charging (${last.battery.powerSource.toUpperCase()})` : last.battery.status}
            {last.battery.health !== "good" && last.battery.health !== "unknown" && <b className="warn-inline"> health: {last.battery.health}</b>}
          </span>
        )}
        <div className="spacer" />
        <select value={interval} onChange={(e) => setIntervalSec(Number(e.target.value))} aria-label="Refresh interval">
          {[1, 3, 5, 10].map((s) => <option key={s} value={s}>every {s}s</option>)}
        </select>
        <button onClick={() => setPoints([])}>Reset chart</button>
      </div>
      {error && <div className="error" role="alert">{error}</div>}
      <div className="healthgrid">
        <section className="card">
          <h3>Battery temperature</h3>
          <div className="big">{fmt(batt?.now)}</div>
          <Sparkline values={points.map((p) => p.batt)} label="Battery temperature" />
          <div className="muted">session min {fmt(batt?.min)} · max {fmt(batt?.max)}</div>
        </section>
        <section className="card">
          <h3>Hottest sensor {last?.hottest && <span className="muted">({last.hottest.name})</span>}</h3>
          <div className="big">{fmt(hot?.now)}</div>
          <Sparkline values={points.map((p) => p.hot)} label="Hottest sensor" />
          <div className="muted">session min {fmt(hot?.min)} · max {fmt(hot?.max)}</div>
        </section>
        <section className="card">
          <h3>CPU speed (throttling)</h3>
          {last?.cpu.length ? (
            <div className="cpubars">
              {last.cpu.slice(0, 16).map((c) => {
                const pct = Math.round((c.curKhz / c.maxKhz) * 100);
                return (
                  <div key={c.core} className="cpubar" title={`${c.core}: ${(c.curKhz / 1000).toFixed(0)} of ${(c.maxKhz / 1000).toFixed(0)} MHz`}>
                    <span className="mono">{c.core}</span>
                    <div className="track"><div className="fill" style={{ width: `${pct}%` }} /></div>
                    <span className="num">{pct}%</span>
                  </div>
                );
              })}
            </div>
          ) : <p className="muted">Not available on this phone.</p>}
          <p className="muted small">Low numbers while busy can mean the phone is slowing itself down because of heat.</p>
        </section>
        <section className="card">
          <h3>All sensors</h3>
          {sensors.length ? (
            <table className="sensors"><tbody>
              {sensors.slice(0, 14).map((s) => <tr key={`${s.source}-${s.name}`}><td className="mono">{s.name}</td><td className="muted">{s.source === "hal" ? "system" : "kernel"}</td><td className="num">{s.tempC.toFixed(1)}°C</td></tr>)}
            </tbody></table>
          ) : <p className="muted">No temperature sensors could be read.</p>}
        </section>
      </div>
      {last && last.missing.length > 0 && <p className="note">Not available on this phone: {last.missing.join(", ")}. Android versions and makers expose different sensors.</p>}
      <p className="note">
        Heat mostly comes from a bright screen, charging while using the phone, and apps busy in the background. This tool cannot change the CPU speed or fan-like settings without root.
        {" "}<button className="linklike" onClick={onShowRunning}>See which apps are using the CPU</button>
      </p>
    </div>
  );
}
