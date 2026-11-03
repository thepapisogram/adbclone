import { useCallback, useEffect, useMemo, useState } from "react";
import { ipc } from "../../lib/ipc";
import type { Action, AppInfo, RunningResponse } from "../../lib/types";
import { SafetyTag } from "../../components/SafetyTag";
import { AppIcon } from "../../components/AppIcon";

type SortKey = "rss" | "cpu" | "name";

const mb = (kb: number) => (kb >= 1_048_576 ? `${(kb / 1_048_576).toFixed(2)} GB` : `${(kb / 1024).toFixed(0)} MB`);

interface Props {
  serial: string;
  /** Wi-Fi connections are slower, so poll less often by default. */
  wireless?: boolean;
  apps: AppInfo[];
  active: boolean;
  onRequest: (action: Action, names: string[]) => void;
}

export function Running({ serial, wireless, apps, active, onRequest }: Props) {
  const [data, setData] = useState<RunningResponse | null>(null);
  const [error, setError] = useState("");
  const [interval, setIntervalSec] = useState(wireless ? 5 : 3);
  useEffect(() => { setIntervalSec(wireless ? 5 : 3); }, [wireless]);
  const [hideSystem, setHideSystem] = useState(true);
  const [sort, setSort] = useState<{ key: SortKey; dir: 1 | -1 }>({ key: "rss", dir: -1 });
  const [selected, setSelected] = useState<Set<string>>(new Set());

  const byName = useMemo(() => new Map(apps.map((a) => [a.name, a])), [apps]);

  const poll = useCallback(async () => {
    try { setData(await ipc.listRunning(serial)); setError(""); }
    catch (e) { setError(String(e)); }
  }, [serial]);

  // Poll only while this tab is visible and a device is selected.
  useEffect(() => {
    if (!active || !serial) return;
    let stopped = false;
    let timer: ReturnType<typeof setTimeout>;
    const loop = async () => {
      await poll();
      if (!stopped) timer = setTimeout(loop, interval * 1000);
    };
    setData(null);
    loop();
    return () => { stopped = true; clearTimeout(timer); };
  }, [active, serial, interval, poll]);

  const rows = useMemo(() => {
    if (!data) return [];
    const list = data.rows
      .filter((r) => r.package !== null)
      .filter((r) => !hideSystem || !byName.get(r.package!)?.system)
      .map((r) => ({ ...r, label: byName.get(r.package!)?.label ?? r.package!, system: byName.get(r.package!)?.system ?? false }));
    list.sort((a, b) => {
      const c = sort.key === "rss" ? a.rssKb - b.rssKb : sort.key === "cpu" ? (a.cpu ?? -1) - (b.cpu ?? -1) : a.label.localeCompare(b.label);
      return c * sort.dir;
    });
    return list;
  }, [data, hideSystem, sort, byName]);

  const sys = data?.rows.find((r) => r.package === null);
  const toggle = (n: string) => setSelected((s) => { const x = new Set(s); x.has(n) ? x.delete(n) : x.add(n); return x; });
  const sortBy = (key: SortKey) => setSort((s) => ({ key, dir: s.key === key ? (-s.dir as 1 | -1) : key === "name" ? 1 : -1 }));
  const arrow = (k: SortKey) => (sort.key === k ? (sort.dir === 1 ? " ▲" : " ▼") : "");
  const names = [...selected].filter((n) => rows.some((r) => r.package === n));
  const usedKb = data ? data.memTotalKb - data.memAvailableKb : 0;
  const pct = data && data.memTotalKb ? Math.round((usedKb / data.memTotalKb) * 100) : 0;

  if (!serial) return <div className="empty-state">Select a device to see running apps.</div>;

  return (
    <div className="running">
      <div className="actions">
        <span>
          {data ? <>RAM used <b>{mb(usedKb)}</b> of {mb(data.memTotalKb)} ({pct}%) · {data.cores} cores · {rows.length} apps running</> : "Loading…"}
          {sys && <> · system processes {mb(sys.rssKb)}{sys.cpu != null && `, ${sys.cpu.toFixed(1)}% CPU`}</>}
        </span>
        <div className="spacer" />
        <label className="inline"><input type="checkbox" checked={hideSystem} onChange={(e) => setHideSystem(e.target.checked)} /> Hide system apps</label>
        <select value={interval} onChange={(e) => setIntervalSec(Number(e.target.value))} aria-label="Refresh interval">
          {[1, 3, 5, 10].map((s) => <option key={s} value={s}>every {s}s</option>)}
        </select>
        <button disabled={!names.length} onClick={() => onRequest("stop", names)}>Force stop ({names.length})</button>
        <button disabled={!names.length} onClick={() => onRequest("disable", names)}>Disable</button>
        <button className="danger" disabled={!names.length} onClick={() => onRequest("uninstall", names)}>Uninstall</button>
      </div>
      {error && <div className="error" role="alert">{error}</div>}
      <div className="tablewrap">
        <table>
          <thead>
            <tr>
              <th className="chk" />
              <th onClick={() => sortBy("name")}>App{arrow("name")}</th>
              <th>Package</th>
              <th onClick={() => sortBy("rss")} className="num">RAM{arrow("rss")}</th>
              <th onClick={() => sortBy("cpu")} className="num" title="Percent of one core">CPU %{arrow("cpu")}</th>
              <th className="num">Procs</th>
              <th title="Is it safe to disable?">Disable</th>
              <th title="Is it safe to uninstall?">Uninstall</th>
            </tr>
          </thead>
          <tbody>
            {rows.map((r) => (
              <tr key={r.package} className={selected.has(r.package!) ? "sel" : ""} onClick={() => toggle(r.package!)} title={r.description ?? undefined}>
                <td className="chk"><input type="checkbox" checked={selected.has(r.package!)} onChange={() => toggle(r.package!)} onClick={(e) => e.stopPropagation()} /></td>
                <td><span className="namecell"><AppIcon name={r.package!} />{r.label}</span></td>
                <td className="mono">{r.package}</td>
                <td className="num">{mb(r.rssKb)}</td>
                <td className="num">{r.cpu == null ? "—" : r.cpu.toFixed(1)}</td>
                <td className="num">{r.processes}</td>
                <td><SafetyTag kind="Disable" value={r.disableSafety} /></td>
                <td><SafetyTag kind="Uninstall" value={r.uninstallSafety} /></td>
              </tr>
            ))}
            {data && !rows.length && <tr><td colSpan={8} className="empty">No matching running apps.</td></tr>}
          </tbody>
        </table>
      </div>
      <p className="note">RAM is resident memory per app (all its processes); CPU is a snapshot between refreshes. Ratings are unreviewed guidance, not guarantees.</p>
    </div>
  );
}
