import { useCallback, useEffect, useMemo, useState } from "react";
import { ipc } from "../../lib/ipc";
import type { Action, AppInfo, AppStorage, Usage } from "../../lib/types";
import { AppIcon } from "../../components/AppIcon";
import { useSettings } from "../../lib/settings";
import { unusedApps, usageMap } from "./unused";

const errText = (e: unknown) => String(e instanceof Error ? e.message : e);
const size = (b: number) => (b >= 1_073_741_824 ? `${(b / 1_073_741_824).toFixed(2)} GB` : b >= 1_048_576 ? `${Math.round(b / 1_048_576)} MB` : b > 0 ? `${Math.max(1, Math.round(b / 1024))} KB` : "—");

interface Props {
  serial: string;
  apps: AppInfo[];
  /** Apps the phone is using for a main job (never offered as unused). */
  protections: ReadonlyMap<string, string>;
  busy: boolean;
  active: boolean;
  onRequest: (action: Action, names: string[]) => void;
}

export function Storage({ serial, apps, protections, busy, active, onRequest }: Props) {
  const { plain } = useSettings();
  const [sizes, setSizes] = useState<AppStorage[] | null>(null);
  const [usage, setUsage] = useState<Usage[] | null>(null);
  const [error, setError] = useState("");
  const [includeSystem, setIncludeSystem] = useState(false);
  const [days, setDays] = useState(60);
  const [picked, setPicked] = useState<Set<string>>(new Set());
  const [cleanMsg, setCleanMsg] = useState("");
  const [cleaning, setCleaning] = useState(false);

  const load = useCallback(() => {
    if (!serial) return;
    setSizes(null); setUsage(null); setError("");
    ipc.storage(serial).then((r) => setSizes(r.available ? r.apps : [])).catch((e) => { setSizes([]); setError(errText(e)); });
    ipc.usage(serial).then((r) => setUsage(r.available ? r.entries : [])).catch((e) => { setUsage([]); setError(errText(e)); });
  }, [serial]);
  useEffect(() => { if (active) load(); }, [active, load]);

  const byName = useMemo(() => new Map(apps.map((a) => [a.name, a])), [apps]);
  const sizeOf = useMemo(() => new Map((sizes ?? []).map((s) => [s.package, s])), [sizes]);
  const totalOf = (s: AppStorage) => s.appBytes + s.dataBytes + s.cacheBytes;

  const biggest = useMemo(() => (sizes ?? []).filter((s) => { const a = byName.get(s.package); return a && (includeSystem || !a.system); }).slice(0, 30), [sizes, byName, includeSystem]);
  const cacheTotal = useMemo(() => (sizes ?? []).reduce((n, s) => n + s.cacheBytes, 0), [sizes]);

  const haveUsage = !!usage && usage.length > 0;
  const unused = useMemo(
    () => (haveUsage ? unusedApps(apps, usageMap(usage!), { days, includeSystem, protectedPkgs: new Set(protections.keys()) }) : []),
    [apps, usage, haveUsage, days, includeSystem, protections],
  );
  useEffect(() => { setPicked((p) => new Set([...p].filter((n) => unused.some((u) => u.app.name === n)))); }, [unused]);
  const toggle = (n: string) => setPicked((s) => { const x = new Set(s); x.has(n) ? x.delete(n) : x.add(n); return x; });
  const names = [...picked];
  const nameOf = (a: AppInfo) => a.label;

  if (!serial) return <div className="empty-state">Select a device to see what uses its space.</div>;

  return (
    <div className="storage">
      {error && <div className="error" role="alert">{error}</div>}

      <section className="block">
        <h3>What uses space</h3>
        {sizes === null ? <p className="muted">Asking the phone…</p> : sizes.length === 0 ? (
          <p className="muted">This phone did not report app sizes (some Android versions and makers keep them private). You can still clear caches below.</p>
        ) : (
          <>
            <div className="row">
              <label className="inline"><input type="checkbox" checked={includeSystem} onChange={(e) => setIncludeSystem(e.target.checked)} /> Include apps that came with the phone</label>
              <div className="spacer" />
              <span className="muted">Caches add up to <b>{size(cacheTotal)}</b></span>
              <button disabled={cleaning} onClick={async () => { setCleaning(true); setCleanMsg("Clearing…"); try { await ipc.tuning.trimCaches(serial); setCleanMsg("Caches cleared. Apps may open a little slower the first time."); load(); } catch (e) { setCleanMsg(errText(e)); } finally { setCleaning(false); } }}>Clear all app caches</button>
            </div>
            {cleanMsg && <p className="muted small">{cleanMsg}</p>}
            <table className="storagetable">
              <thead><tr><th>App</th><th className="num">App</th><th className="num">Its data</th><th className="num">Cache</th><th className="num">Total</th></tr></thead>
              <tbody>
                {biggest.map((s) => {
                  const a = byName.get(s.package)!;
                  return (
                    <tr key={s.package} className="static">
                      <td><span className="namecell"><AppIcon name={s.package} />{nameOf(a)}{!plain && <span className="mono muted"> {s.package}</span>}</span></td>
                      <td className="num">{size(s.appBytes)}</td><td className="num">{size(s.dataBytes)}</td><td className="num">{size(s.cacheBytes)}</td><td className="num"><b>{size(totalOf(s))}</b></td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
            <p className="muted small">Sizes are what the phone measured last, so they can be a little out of date.</p>
          </>
        )}
      </section>

      <section className="block">
        <h3>Apps you have not opened lately</h3>
        {usage === null ? <p className="muted">Asking the phone…</p> : !haveUsage ? (
          <p className="muted">This phone did not report when apps were last opened (it may keep that private, or have no history yet). Nothing is guessed.</p>
        ) : (
          <>
            <div className="row">
              <label className="inline">Not opened for{" "}
                <select value={days} onChange={(e) => setDays(Number(e.target.value))} aria-label="Not opened for">
                  {[30, 60, 90, 180].map((d) => <option key={d} value={d}>{d} days</option>)}
                </select>
              </label>
              <label className="inline"><input type="checkbox" checked={includeSystem} onChange={(e) => setIncludeSystem(e.target.checked)} /> Include apps that came with the phone</label>
              <div className="spacer" />
              <button disabled={busy || names.length === 0} onClick={() => onRequest("disable", names)}>Disable {names.length || ""}</button>
              <button className="danger" disabled={busy || names.length === 0} onClick={() => onRequest("uninstall", names)}>Uninstall {names.length || ""}</button>
            </div>
            <p className="muted small">The phone only keeps a limited history, so "no record" means it has not seen the app opened lately, not that it never was. Check before removing. A safety snapshot is saved first.</p>
            {unused.length === 0 ? <p className="status ok">Nothing here: every app was opened within {days} days.</p> : (
              <table className="storagetable" aria-label="Unused apps">
                <tbody>
                  {unused.map(({ app, daysAgo }) => (
                    <tr key={app.name} className={picked.has(app.name) ? "sel" : ""} onClick={() => toggle(app.name)}>
                      <td className="chk"><input type="checkbox" checked={picked.has(app.name)} onChange={() => toggle(app.name)} onClick={(e) => e.stopPropagation()} aria-label={`Select ${app.label}`} /></td>
                      <td><span className="namecell"><AppIcon name={app.name} />{app.label}{!plain && <span className="mono muted"> {app.name}</span>}</span></td>
                      <td className="muted">{daysAgo === null ? "no record of it being opened" : `last opened ${daysAgo} day${daysAgo === 1 ? "" : "s"} ago`}</td>
                      <td className="num">{sizeOf.get(app.name) ? size(totalOf(sizeOf.get(app.name)!)) : ""}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )}
          </>
        )}
      </section>
    </div>
  );
}
